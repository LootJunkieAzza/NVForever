//! Slot machines in co-op (queue item 170): the host decides every spin.
//!
//! Each player plays their own machine, with their own chips. When a guest pulls the lever
//! its bet is taken at once and it asks the host for the roll
//! ([`crate::coop_tables::SlotsAction::Spin`], the table the machine); the host rolls the
//! reels with its own generator, or to the test aid's line when the guest's spin carries
//! one, and answers with the [`SlotsPlay`] (the stops and what the line pays,
//! [`crate::coop_tables::SlotsAction::Roll`], the table the player it is for). The guest's
//! reels then turn to the host's stops exactly as a spin of its own does, and the line is
//! paid from its own session, so the winnings go to the player who spun. The host's own
//! spins are rolled where they are made. The others see the reels turn and stop on that
//! machine through the slots view (`super::machines`), which carries the same stops.
//!
//! [design] A guest that hears nothing for [`ROLL_WAIT`] (a host of an older build, a
//! lost packet) rolls the spin itself, with a warning, so the menu never hangs.

use super::{rules, sounds, Phase, Session, SlotsScreen, SlotsSpin, SlotsStats, REELS};
use crate::coop_tables::{game, SlotsAction, SlotsPlay, Tables};
use crate::pause::AidTime;
use crate::script::ScriptState;
use crate::sfx::PlaySound;
use bevy::prelude::*;
use fnv_sim::FormId;
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

/// How long a guest waits for the host's roll before rolling itself, seconds [design: the
/// actions go out every frame; a round trip on a LAN or Steam relay is well under this].
pub const ROLL_WAIT: f32 = 1.5;

/// A spin asked of the host and not answered yet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Asked {
    /// When it was asked (aid seconds).
    pub since: f32,
    /// The spin's number at this machine (the session's spins + 1).
    pub spin: u32,
    pub bet: u32,
    /// The test aid's line, sent with it.
    pub forced: Option<[u8; REELS]>,
}

/// Whether this player's spin is the host's to roll: a co-op guest at a placed machine.
pub fn asks_host(t: &Tables, s: &Session) -> bool {
    t.on && !t.host && t.me.is_some() && s.machine.is_some()
}

/// Ask the host for the spin (the bet is already taken).
pub fn ask(t: &mut Tables, s: &mut Session, forced: Option<[u8; REELS]>, now: f32) {
    let Some(m) = s.machine else { return };
    let a = Asked { since: now, spin: s.spins + 1, bet: s.bet, forced };
    t.send(m.0, game::SLOTS, SlotsAction::Spin { machine: m.0, casino: s.casino.0, spin: a.spin, bet: a.bet, forced }.encode());
    s.payouts = false;
    s.status = None;
    s.asked = Some(a);
    info!("slots coop: spin {} at machine {:08X} asked of the host: bet {}, {} chips left", a.spin, m.0, a.bet, s.chips);
}

/// The roll for a spin: the test aid's line when given (the first stop of each symbol),
/// else a random stop on each reel.
pub fn roll(rng: &mut ChaCha8Rng, reel: &[u8], forced: Option<[u8; REELS]>) -> [usize; REELS] {
    match forced {
        Some(l) => std::array::from_fn(|i| rules::stop_of(reel, l[i]).unwrap_or(0)),
        None => rules::spin(rng, reel.len().max(1)),
    }
}

/// The co-op side of the slots, every frame: the host answers the guests' spins; a guest
/// starts the spin the host rolled for it (or, unanswered, rolls it itself).
#[allow(clippy::too_many_arguments)]
pub(super) fn exchange(
    tables: Option<ResMut<Tables>>,
    mut screen: ResMut<SlotsScreen>,
    script: Option<Res<ScriptState>>,
    time: AidTime,
    mut stats: ResMut<SlotsStats>,
    (mut sounds, mut spin_out): (MessageWriter<PlaySound>, MessageWriter<SlotsSpin>),
    mut rng: Local<Option<ChaCha8Rng>>,
) {
    let now = time.elapsed_secs();
    let mut rolled: Option<SlotsPlay> = None;
    if let Some(mut t) = tables {
        let me = t.me;
        let host = t.host;
        let mut out: Vec<(u32, Vec<u8>)> = Vec::new();
        for (from, _table, blob) in t.take_actions(game::SLOTS) {
            let Some(a) = SlotsAction::decode(&blob) else { continue };
            match a {
                SlotsAction::Spin { machine, casino, spin, bet, forced } if host => {
                    let Some(def) = script.as_deref().and_then(|s| s.rt.as_ref()).and_then(|rt| rt.game.data.casinos.get(&FormId(casino))) else {
                        warn!("slots coop: {} asked a spin at {casino:08X}, not a casino here", t.name(from));
                        continue;
                    };
                    let reel = rules::reel(&def.reel_stops);
                    let g = rng.get_or_insert_with(|| {
                        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
                        ChaCha8Rng::seed_from_u64(t ^ 0x5107_5107)
                    });
                    let to = roll(g, &reel, forced);
                    let line = to.map(|x| reel.get(x % reel.len().max(1)).copied().unwrap_or(0));
                    let play = SlotsPlay { machine, player: from, spin, bet, to: to.map(|x| x as u8), multiple: rules::multiple(line) };
                    let names = line.map(|x| rules::NAMES[x as usize]);
                    info!(
                        "slots coop: rolled spin {spin} for {} at machine {machine:08X}: bet {bet}, the reels stop on {} {} {}, pays {} x{}",
                        t.name(from),
                        names[0],
                        names[1],
                        names[2],
                        play.multiple,
                        if forced.is_some() { " (its test line)" } else { "" }
                    );
                    out.push((u32::from(from), SlotsAction::Roll(play).encode()));
                }
                SlotsAction::Roll(p) if Some(p.player) == me && !host => rolled = Some(p),
                _ => {}
            }
        }
        for (to, blob) in out {
            t.send(to, game::SLOTS, blob);
        }
    }
    let Some(s) = screen.0.as_mut() else { return };
    let Some(a) = s.asked else { return };
    let to = match rolled {
        Some(p) if p.spin == a.spin && s.machine.map(|m| m.0) == Some(p.machine) => {
            let to = p.to.map(|x| x as usize);
            let here = rules::multiple(to.map(|x| s.reel.get(x % s.reel.len().max(1)).copied().unwrap_or(0)));
            if here != p.multiple {
                warn!("slots coop: the host's line pays {} x, this game's reel reads {here} x (casino records differ?)", p.multiple);
            }
            info!("slots coop: spin {} rolled by the host after {:.2}s", a.spin, now - a.since);
            to
        }
        Some(p) => {
            info!("slots coop: a roll for spin {} at {:08X} ignored (waiting for spin {})", p.spin, p.machine, a.spin);
            return;
        }
        None if now - a.since > ROLL_WAIT => {
            warn!("slots coop: no roll from the host for spin {} after {ROLL_WAIT}s; rolled here", a.spin);
            roll(&mut s.rng, &s.reel, a.forced)
        }
        None => return,
    };
    debug_assert_eq!(s.phase, Phase::Betting);
    s.asked = None;
    spin_out.write(s.begin(to, now));
    stats.spins += 1;
    let names = s.line().map(|x| rules::NAMES[x as usize]);
    info!("slots: spin: bet {}, {} chips left; the reels will stop on {} {} {}", a.bet, s.chips, names[0], names[1], names[2]);
    // The pull is heard (and the arm drawn, the camera moved) when the reels start.
    sounds.write(PlaySound { sound: sounds::PULL, at: None });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_forced_line_rolls_to_its_first_stops() {
        let reel: Vec<u8> = (0..14).map(|i| (i % 7) as u8).collect();
        let mut g = ChaCha8Rng::seed_from_u64(1);
        assert_eq!(roll(&mut g, &reel, Some([6, 6, 6])), [6, 6, 6]);
        assert_eq!(roll(&mut g, &reel, Some([0, 1, 2])), [0, 1, 2]);
        let r = roll(&mut g, &reel, None);
        assert!(r.iter().all(|&x| x < reel.len()));
    }
}
