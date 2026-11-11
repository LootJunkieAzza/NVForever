//! The slot machines in use now, by placed machine (item 170): this player's own (the open
//! menu's [`super::Session::machine`]) and, in co-op, the other players' (their table
//! views, `crate::coop_tables::SlotsView`, which name the machine, the stops and how long
//! the spin has turned). nvforever `slots_view.rs` turns the reels of each on the machine
//! in the world, so a passer-by sees the same spin and result as the player at it.
//!
//! A spin heard from another player starts where that player's started: the view says how
//! long ago, and the reels turn from the stops last seen [design: the views come every
//! 0.25 s; a view missed shows the reels at the new stops].

use super::{reel_positions, Phase, SlotsScreen, REEL_STOPS};
use crate::coop_tables::{game, SlotsView, Tables, VIEW_STALE};
use crate::pause::AidTime;
use crate::script::ScriptState;
use bevy::prelude::*;
use fnv_game::slots::{self as rules, REELS};
use fnv_sim::FormId;
use std::collections::HashMap;

/// Whose machine it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Player {
    /// This player's, the open menu's.
    Me,
    /// Another co-op player's (its id).
    Other(u8),
}

/// A machine in use and where its reels are.
#[derive(Debug, Clone, PartialEq)]
pub struct MachineSpin {
    pub player: Player,
    pub casino: FormId,
    /// The symbols' pictures (CSNO ICON), under `textures\`, symbol 1 to W.
    pub symbols: Vec<String>,
    /// The reel's stops: the symbol (0 to 6) at each.
    pub reel: Vec<u8>,
    /// Where the reels stand (or will stop).
    pub stops: [usize; REELS],
    /// The spin turning: when it started (aid seconds) and the stops it left.
    pub spinning: Option<(f32, [usize; REELS])>,
    /// Spins made so far (a new one starts the reels).
    pub spin: u32,
    /// What the last spin's line paid, times the bet, once its reels stopped.
    pub paid: Option<u32>,
}

impl MachineSpin {
    /// Where each reel stands at `now`, in stops.
    pub fn positions(&self, now: f32) -> [f32; REELS] {
        match self.spinning {
            Some((since, from)) => reel_positions(self.reel.len(), from, self.stops, now - since),
            None => self.stops.map(|s| s as f32),
        }
    }

    /// The symbols on the line once stopped.
    pub fn line(&self) -> [u8; REELS] {
        let n = self.reel.len().max(1);
        self.stops.map(|s| self.reel.get(s % n).copied().unwrap_or(0))
    }
}

/// The machines in use, by placed reference.
#[derive(Resource, Default, Debug)]
pub struct SlotMachines(pub HashMap<FormId, MachineSpin>);

/// Keep [`SlotMachines`]: this player's machine from the open menu, the others' from their
/// table views.
pub fn track(screen: Res<SlotsScreen>, tables: Option<Res<Tables>>, script: Option<Res<ScriptState>>, time: AidTime, real: Res<Time<Real>>, mut machines: ResMut<SlotMachines>) {
    let now = time.elapsed_secs();
    let mut keep: Vec<FormId> = Vec::new();
    if let Some((m, s)) = screen.0.as_ref().and_then(|s| s.machine.map(|m| (m, s))) {
        keep.push(m);
        let spinning = match s.phase {
            Phase::Spinning { since, from, .. } => Some((since, from)),
            _ => None,
        };
        let paid = matches!(s.phase, Phase::Paid { .. }).then(|| rules::multiple(s.line()));
        let entry = MachineSpin { player: Player::Me, casino: s.casino, symbols: s.symbols.clone(), reel: s.reel.clone(), stops: s.stops, spinning, spin: s.spins, paid };
        if machines.0.get(&m) != Some(&entry) {
            machines.0.insert(m, entry);
        }
    }
    if let (Some(t), Some(rt)) = (tables.as_deref(), script.as_deref().and_then(|s| s.rt.as_ref())) {
        for (&id, (casino, g, blob, heard)) in &t.seen {
            if *g != game::SLOTS || real.elapsed_secs() - heard > VIEW_STALE {
                continue;
            }
            let Some(v) = SlotsView::decode(blob) else { continue };
            let Some(m) = v.machine.filter(|m| *m != 0).map(FormId) else { continue };
            if keep.contains(&m) {
                continue;
            }
            keep.push(m);
            let Some(def) = rt.game.data.casinos.get(&FormId(*casino)) else { continue };
            let stops = v.stops.map(|s| s as usize);
            let old = machines.0.get(&m).filter(|o| o.player == Player::Other(id));
            let spinning = if v.spinning {
                match old {
                    // The same spin: keep its start.
                    Some(o) if o.spin == v.spin && o.spinning.is_some() => o.spinning,
                    Some(o) => Some((now - v.elapsed, o.stops)),
                    None => Some((now - v.elapsed, stops)),
                }
            } else {
                None
            };
            let paid = (!v.spinning && v.spin > 0).then(|| rules::multiple(v.line));
            let entry = MachineSpin { player: Player::Other(id), casino: FormId(*casino), symbols: def.symbols.clone(), reel: rules::reel(&def.reel_stops), stops, spinning, spin: v.spin, paid };
            if old.is_none() {
                info!("slots: {} plays the slot machine {:08X}", t.name(id), m.0);
            }
            if machines.0.get(&m) != Some(&entry) {
                machines.0.insert(m, entry);
            }
        }
    }
    // A spin ends when its last reel has stopped.
    for e in machines.0.values_mut() {
        if e.spinning.is_some_and(|(since, _)| now - since > REEL_STOPS[REELS - 1] + 0.5) && e.player != Player::Me {
            e.spinning = None;
        }
    }
    if machines.0.len() != keep.len() {
        machines.0.retain(|m, _| keep.contains(m));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spin_turns_then_stands() {
        let reel: Vec<u8> = (0..14).map(|i| (i % 7) as u8).collect();
        let mut m = MachineSpin { player: Player::Me, casino: FormId(1), symbols: vec![], reel, stops: [3, 4, 5], spinning: Some((10.0, [0, 0, 0])), spin: 1, paid: None };
        assert_eq!(m.positions(10.0), [0.0, 0.0, 0.0]);
        let end = m.positions(20.0);
        assert_eq!(end.map(|p| p.round() as usize % 14), [3, 4, 5]);
        m.spinning = None;
        assert_eq!(m.positions(20.0), [3.0, 4.0, 5.0]);
        assert_eq!(m.line(), [3, 4, 5]);
    }
}
