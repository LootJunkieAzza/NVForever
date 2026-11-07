//! The other co-op players as this player's companions (queue item 65): each game tells
//! the others its player's Charisma and whether its Ferocious Loyalty acts now, and puts
//! what they give into `Game::coop_from_others` (`fnv_game::coop_perks::FromOthers`:
//! Companion Nerve from the highest Charisma of the others, Ferocious Loyalty's Damage
//! Resistance while another player with it is below half health). Combat reads it as a
//! companion's (`crate::combat`: `companion_nerve`, `target_armor`, `remote_hits`).
//!
//! A player's own perks never count for itself; a player gone from the session, or not
//! heard for [`STALE`] seconds, gives nothing; out of a session it is all 0 [design].

use crate::coop_tables::{game, Tables};
use crate::script::ScriptState;
use crate::PlaySet;
use bevy::prelude::*;
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
}

fn exchange(time: Res<Time<Real>>, tables: Option<ResMut<Tables>>, script: Option<ResMut<ScriptState>>, mut seen: Local<Seen>) {
    let Some(mut script) = script else { return };
    let Some(rt) = script.rt.as_mut() else { return };
    let local = time.elapsed_secs_f64();
    let Some(mut tables) = tables.filter(|t| t.on) else {
        if seen.last.is_some() || rt.game.coop_from_others != Default::default() {
            *seen = Seen::default();
            rt.game.coop_from_others = Default::default();
            info!("co-op companions: out of the session: nothing from other players");
        }
        return;
    };
    let mine = fnv_game::coop_perks::my_side(&mut rt.game);
    if seen.last != Some(mine) || local - seen.sent >= EVERY {
        if seen.last != Some(mine) {
            info!("co-op companions: this player tells the others Charisma {}, Ferocious Loyalty {}", mine.0, if mine.1 { "acting" } else { "not acting" });
        }
        seen.sent = local;
        seen.last = Some(mine);
        tables.send(0, game::PERKS, vec![mine.0, u8::from(mine.1)]);
    }
    for (from, _, blob) in tables.take_actions(game::PERKS) {
        if let [charisma, loyal, ..] = blob[..] {
            seen.others.insert(from, (charisma, loyal != 0, local));
        }
    }
    let me = tables.me;
    seen.others.retain(|p, (.., at)| Some(*p) != me && tables.names.contains_key(p) && local - *at < STALE);
    let given = fnv_game::coop_perks::from_others(seen.others.values().map(|(c, l, _)| (*c, *l)), fnv_game::coop_perks::loyalty_dr(rt.game.data));
    if given != seen.given {
        info!("co-op companions: from the other players: Companion Nerve {:+.0} %, Damage Resistance {:+.0} (Ferocious Loyalty)", given.nerve * 100.0, given.dr);
        seen.given = given;
    }
    rt.game.coop_from_others = given;
}
