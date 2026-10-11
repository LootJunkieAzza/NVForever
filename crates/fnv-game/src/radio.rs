//! Radios in the runtime (docs/spec-notes/radio.md): which receivers are switched on,
//! whether a station broadcasts, and the station's programme as dialogue: which line it
//! says next, with the line's begin and end result scripts. No engine types; how a
//! receiver sounds is the engine's (`nvf-play` `radio.rs`).
//!
//! - **A receiver** (an activator whose base names a radio station) starts switched off
//!   and each default activation switches it on or off [hypothesis: the receivers'
//!   scripts — VintageRadioStartOnSCRIPT activates its own reference once on load, so a
//!   radio "starts on" only by being activated; radio.md §4]. The runtime counts the
//!   default activations of every reference whose activation is "activated"
//!   (activators, furniture), as scripts can activate a radio on the first frame, before
//!   the engine could say which references are receivers; the engine reads the counts of
//!   its receivers ([`Game::radio_on`]).
//! - **A station broadcasts** when SetBroadcastState last said so, else as its base's
//!   Continuous Broadcast flag says [documented: GECK wiki Radio Stations, "the default
//!   state … is set using the Continuous Broadcast checkbox"; hypothesis for which
//!   header bit that is, radio.md §2]. `GetBroadcastState` is not changed by this (it
//!   still reads 0 for a station never set; radio.md §8).
//! - **The programme**: a station says lines as an actor does, with its placed reference
//!   as the speaker, so the radio quests' own conditions (GetIsID of the station) pick
//!   whose lines it has [observed: every radio quest of the masters gates its lines that
//!   way, radio.md §3]. It starts with RadioHello [documented: GECK wiki Radio Tab and
//!   Radio Stations: RadioHello starts radio conversations as HELLO does]. After a line
//!   it goes on with the topics the line links to (TCLT): one of the linked topics
//!   that has a line the station can say, picked at random [hypothesis: radio.md §5];
//!   with none, back to RadioHello [hypothesis]. Within a topic the line is picked as
//!   for any speaker ([`Game::select_line`]: quest priority, stored order, Random pools).

use crate::game::{Game, RunCtx};
use fnv_sim::FormId;
use rand_core::RngCore;
use std::collections::BTreeMap;

/// The topic radio programmes start from [documented: GECK wiki Radio Tab].
pub const RADIO_HELLO: &str = "RadioHello";

#[derive(Debug, Clone, Default)]
pub struct Radios {
    /// Default activations whose outcome was "activated", by reference.
    pub activations: BTreeMap<FormId, u32>,
}

impl Game<'_> {
    /// A default activation of `target` that "activated" it.
    pub(crate) fn radio_activated(&mut self, target: FormId) {
        *self.radio.activations.entry(target).or_default() += 1;
    }

    /// Whether the receiver `r` is switched on: activated an odd number of times.
    pub fn radio_on(&self, r: FormId) -> bool {
        self.radio.activations.get(&r).is_some_and(|n| n % 2 == 1)
    }

    /// How many times the receiver `r` was switched on or off.
    pub fn radio_switches(&self, r: FormId) -> u32 {
        self.radio.activations.get(&r).copied().unwrap_or(0)
    }

    /// Whether the station whose placed reference is `station` broadcasts: SetBroadcastState's
    /// last word, else `continuous` (the base's flag).
    pub fn broadcasting(&self, station: FormId, continuous: bool) -> bool {
        self.extra.get(&station).and_then(|e| e.broadcast).unwrap_or(continuous)
    }

    /// The topic and line the station `station` (a placed reference) says next, after
    /// `after` (the line it said last, `None` at the start). `None`: nothing it can say.
    pub fn radio_next(&mut self, station: FormId, after: Option<FormId>) -> Option<(FormId, FormId)> {
        let data = self.data;
        let listener = Some(FormId::PLAYER_REF);
        if let Some(def) = after.and_then(|i| data.infos.get(&i)) {
            let mut linked: Vec<(FormId, FormId)> = Vec::new();
            for &topic in &def.choices {
                if linked.iter().any(|(t, _)| *t == topic) {
                    continue;
                }
                if let Some(info) = self.select_line(station, listener, topic) {
                    linked.push((topic, info));
                }
            }
            match linked.len() {
                0 => {}
                1 => return Some(linked[0]),
                n => return Some(linked[self.rng.next_u32() as usize % n]),
            }
        }
        let hello = data.by_edid(RADIO_HELLO)?;
        self.select_line(station, listener, hello).map(|i| (hello, i))
    }

    /// The station starts saying `info`: Say Once remembers it and its begin result
    /// script runs, with the station as the script's reference.
    pub fn radio_line_begins(&mut self, station: FormId, info: FormId) {
        let data = self.data;
        let Some(def) = data.infos.get(&info) else { return };
        self.mark_said(station, info, false);
        if let Some(u) = def.begin {
            self.run_result(u, RunCtx { self_ref: Some(station), action_ref: None, quest: def.quest });
        }
    }

    /// The station finished saying `info`: its end result script runs.
    pub fn radio_line_ends(&mut self, station: FormId, info: FormId) {
        let data = self.data;
        let Some(def) = data.infos.get(&info) else { return };
        if let Some(u) = def.end {
            self.run_result(u, RunCtx { self_ref: Some(station), action_ref: None, quest: def.quest });
        }
    }
}
