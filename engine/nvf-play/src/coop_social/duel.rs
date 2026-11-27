//! A duel [design]: pistols only, in an open place; both players
//! stand a few steps apart with their backs turned; a hidden countdown of
//! [`DELAY_MS`] (never shown) holds both still, cameras too; then an orange sweep crosses
//! the screen, the signal to draw. The first to land a shot wins: no damage, the loser is
//! thrown a few feet back.
//!
//! The challenger is the referee: it picks the countdown when both are ready and says who
//! won (the first hit it hears of, its own included). Everything here is plain state,
//! stepped by `super` with what the game sees and what the wire brings.
//!

use fnv_util::words::{tr, trf};
use super::wire::Msg;

/// The countdown's length, milliseconds, from and to [design].
pub const DELAY_MS: (u16, u16) = (2_000, 7_000);
/// How far apart the players stand, game units: at least, at most [design: "a few steps"].
pub const APART: (f32, f32) = (200.0, 1_200.0);
/// A back is turned when the bearing to the other is at least this far from the facing,
/// degrees [design].
pub const BACK_TURNED: f32 = 120.0;
/// Both ready this long (real seconds) before the referee starts the countdown [design].
pub const SETTLE: f32 = 0.6;
/// A ready flag is sent again this often while it holds (real seconds).
pub const READY_EVERY: f32 = 1.0;
/// Positioning gives up after this long; the draw after this long without a hit (real
/// seconds) [design].
pub const POSITION_SECONDS: f32 = 120.0;
pub const DRAW_SECONDS: f32 = 30.0;
/// The result stays this long before the duel is over (real seconds).
pub const OVER_SECONDS: f32 = 4.0;
/// The open space a duel needs around the challenger: game units clear each way along at
/// least one line through it [design: "enough clear ground for both players a few steps
/// apart"].
pub const ROOM_CLEAR: f32 = 350.0;
/// The loser's throw: speed away from the winner and up, game units a second [design: "a
/// few feet back"].
pub const THROW: (f32, f32) = (320.0, 180.0);

/// Whether `line` is one of the lines that says the duel is called off.
pub fn is_called_off(line: &str) -> bool {
    [tr("The duel is called off"), tr("The duel is called off: the connection was lost"), tr("The duel is called off: nobody took position")].contains(&line)
}

/// Why the wheel's Duel cannot be chosen: one reason only, lack of room before the
/// missing pistol [design].
pub fn duel_refusal(room: bool, pistol: bool) -> Option<&'static str> {
    if !room {
        Some(tr("Not enough room to duel here"))
    } else if !pistol {
        Some(tr("Equip a pistol to duel"))
    } else {
        None
    }
}

/// Whether there is room: outdoors, and some line through the player clear for
/// [`ROOM_CLEAR`] both ways. `clear` is the free distance in eight directions, a
/// compass's (0 north, then every 45° clockwise).
pub fn room_for_duel(interior: bool, clear: &[f32; 8]) -> bool {
    !interior && (0..4).any(|i| clear[i] >= ROOM_CLEAR && clear[i + 4] >= ROOM_CLEAR)
}

/// Whether a weapon's animation type is a pistol's (`fnv_combat::data::WeaponStats::anim_type`).
pub fn is_pistol(anim_type: Option<&str>) -> bool {
    anim_type.is_some_and(|t| t.starts_with("Pistol"))
}

/// Where a player stands against the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stance {
    NoPistol,
    TooClose,
    TooFar,
    /// Facing the other (or sideways): turn round.
    Facing,
    Ready,
}

impl Stance {
    /// What the player is told (`None` once ready).
    pub fn hint(self, name: &str) -> Option<String> {
        match self {
            Stance::NoPistol => Some(tr("Equip a pistol for the duel").into()),
            Stance::TooClose => Some(trf("Step away from {name}: a few steps apart", &[("name", &name)])),
            Stance::TooFar => Some(trf("Come closer to {name}: a few steps apart", &[("name", &name)])),
            Stance::Facing => Some(trf("Turn your back to {name}", &[("name", &name)])),
            Stance::Ready => None,
        }
    }
}

/// This player's stance: feet and facing (compass radians: 0 north, π/2 east; X east, Y
/// north) against the other's feet.
pub fn stance(pistol: bool, me: [f32; 3], yaw: f32, them: [f32; 3]) -> Stance {
    if !pistol {
        return Stance::NoPistol;
    }
    let (dx, dy) = (them[0] - me[0], them[1] - me[1]);
    let d = dx.hypot(dy);
    if d < APART.0 {
        return Stance::TooClose;
    }
    if d > APART.1 {
        return Stance::TooFar;
    }
    let bearing = dx.atan2(dy);
    let off = (bearing - yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    if off.abs().to_degrees() >= BACK_TURNED {
        Stance::Ready
    } else {
        Stance::Facing
    }
}

/// The countdown for a random number `r` (0 … u32::MAX): within [`DELAY_MS`].
pub fn delay_for(r: u32) -> u16 {
    let span = (DELAY_MS.1 - DELAY_MS.0) as u64 + 1;
    DELAY_MS.0 + (r as u64 % span) as u16
}

/// The throw for the loser at `loser` from the winner at `winner`: away and up.
pub fn throw(loser: [f32; 3], winner: [f32; 3]) -> [f32; 3] {
    let (dx, dy) = (loser[0] - winner[0], loser[1] - winner[1]);
    let d = dx.hypot(dy);
    let (x, y) = if d > 1e-3 { (dx / d, dy / d) } else { (0.0, 1.0) };
    [x * THROW.0, y * THROW.0, THROW.1]
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Phase {
    /// Getting into position.
    Positioning,
    /// Both still: the draw at `draw_at` (real seconds), never shown.
    Countdown { draw_at: f32 },
    /// The signal came at `since`: shoot.
    Draw { since: f32 },
    /// `winner` (a player id; None: nobody) at `since`.
    Over { winner: Option<u8>, since: f32 },
    /// Finished: drop it.
    Done,
}

/// What the duel asks of the game.
#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    Send(Msg),
    /// A line for the corner.
    Say(String),
    /// The draw: the orange sweep.
    Sweep,
    /// This player lost: thrown back from the winner.
    Thrown,
    /// The other lost: its puppet falls.
    TheyFall,
}

#[derive(Debug, Clone)]
pub struct Duel {
    pub with: u8,
    pub me: u8,
    pub nonce: u16,
    /// The challenger: it starts the countdown and judges the hits.
    pub referee: bool,
    pub phase: Phase,
    since: f32,
    pub mine: Stance,
    theirs_ready: bool,
    sent_ready: Option<(bool, f32)>,
    both_since: Option<f32>,
    hit_sent: bool,
    told: Option<Stance>,
}

impl Duel {
    /// A duel, straight to the positions.
    pub fn new(me: u8, with: u8, nonce: u16, referee: bool, now: f32) -> Duel {
        Duel { with, me, nonce, referee, phase: Phase::Positioning, since: now, mine: Stance::NoPistol, theirs_ready: false, sent_ready: None, both_since: None, hit_sent: false, told: None }
    }

    /// The other player went away (left the session): the duel is called off.
    pub fn other_gone(&mut self) -> Vec<Out> {
        self.called_off(tr("The duel is called off"))
    }

    /// This player's link to the session dropped mid-duel (a guest that lost the host).
    pub fn this_dropped(&mut self) -> Vec<Out> {
        self.called_off(tr("The duel is called off: the connection was lost"))
    }

    fn called_off(&mut self, line: &str) -> Vec<Out> {
        let open = !matches!(self.phase, Phase::Over { .. } | Phase::Done);
        self.phase = Phase::Done;
        if open {
            vec![Out::Say(line.into())]
        } else {
            Vec::new()
        }
    }

    /// Movement and the camera are held (the countdown).
    pub fn frozen(&self) -> bool {
        matches!(self.phase, Phase::Countdown { .. })
    }

    /// Shots count now.
    pub fn drawing(&self) -> bool {
        matches!(self.phase, Phase::Draw { .. })
    }

    /// The other's hits do no damage while the duel lasts, from taking position until it is over.
    pub fn harmless(&self) -> bool {
        !matches!(self.phase, Phase::Done)
    }

    pub fn done(&self) -> bool {
        self.phase == Phase::Done
    }

    /// One frame: this player's stance now, a random number for the countdown.
    pub fn step(&mut self, now: f32, stance: Stance, name: &str, random: u32) -> Vec<Out> {
        let mut out = Vec::new();
        match self.phase {
            Phase::Positioning => {
                self.mine = stance;
                if self.told != Some(stance) {
                    self.told = Some(stance);
                    out.push(Out::Say(stance.hint(name).unwrap_or_else(|| trf("In position. Wait for {name}, then for the signal to draw", &[("name", &name)]))));
                }
                let ready = stance == Stance::Ready;
                if self.sent_ready.is_none_or(|(r, at)| r != ready || now - at >= READY_EVERY) {
                    self.sent_ready = Some((ready, now));
                    out.push(Out::Send(Msg::DuelReady { nonce: self.nonce, ready }));
                }
                if ready && self.theirs_ready {
                    let since = *self.both_since.get_or_insert(now);
                    if self.referee && now - since >= SETTLE {
                        let delay_ms = delay_for(random);
                        out.push(Out::Send(Msg::DuelFreeze { nonce: self.nonce, delay_ms }));
                        self.freeze(now, delay_ms);
                    }
                } else {
                    self.both_since = None;
                }
                if now - self.since >= POSITION_SECONDS {
                    out.push(Out::Say(tr("The duel is called off: nobody took position").into()));
                    out.push(Out::Send(Msg::Quit { nonce: self.nonce }));
                    self.phase = Phase::Done;
                }
            }
            Phase::Countdown { draw_at } if now >= draw_at => {
                self.phase = Phase::Draw { since: now };
                out.push(Out::Sweep);
            }
            Phase::Draw { since } if now - since >= DRAW_SECONDS => {
                if self.referee {
                    out.push(Out::Send(Msg::DuelOver { nonce: self.nonce, winner: 255 }));
                }
                out.extend(self.over(None, now));
            }
            Phase::Over { since, .. } if now - since >= OVER_SECONDS => self.phase = Phase::Done,
            _ => {}
        }
        out
    }

    fn freeze(&mut self, now: f32, delay_ms: u16) {
        let ms = delay_ms.clamp(DELAY_MS.0, DELAY_MS.1);
        self.phase = Phase::Countdown { draw_at: now + ms as f32 / 1000.0 };
    }

    fn over(&mut self, winner: Option<u8>, now: f32) -> Vec<Out> {
        self.phase = Phase::Over { winner, since: now };
        match winner {
            Some(w) if w == self.me => vec![Out::Say(tr("You win the duel").into()), Out::TheyFall],
            Some(_) => vec![Out::Say(tr("You lose the duel").into()), Out::Thrown],
            None => vec![Out::Say(tr("Nobody landed a shot: the duel is a draw").into())],
        }
    }

    /// This player's shot hit the other during the draw.
    pub fn hit(&mut self, now: f32) -> Vec<Out> {
        if !self.drawing() || self.hit_sent {
            return Vec::new();
        }
        self.hit_sent = true;
        if self.referee {
            let mut out = vec![Out::Send(Msg::DuelOver { nonce: self.nonce, winner: self.me })];
            out.extend(self.over(Some(self.me), now));
            out
        } else {
            vec![Out::Send(Msg::DuelHit { nonce: self.nonce })]
        }
    }

    /// A message of this duel from the other.
    pub fn heard(&mut self, m: &Msg, now: f32) -> Vec<Out> {
        match *m {
            Msg::DuelReady { ready, .. } => {
                self.theirs_ready = ready;
                Vec::new()
            }
            Msg::DuelFreeze { delay_ms, .. } if !self.referee && self.phase == Phase::Positioning => {
                self.freeze(now, delay_ms);
                Vec::new()
            }
            // The referee judges: the first hit it hears of wins, unless its own came first.
            Msg::DuelHit { .. } if self.referee && self.drawing() => {
                let mut out = vec![Out::Send(Msg::DuelOver { nonce: self.nonce, winner: self.with })];
                out.extend(self.over(Some(self.with), now));
                out
            }
            // A hit can land a moment before this side has seen its own countdown end.
            Msg::DuelHit { .. } if self.referee && matches!(self.phase, Phase::Countdown { .. }) => {
                let mut out = vec![Out::Send(Msg::DuelOver { nonce: self.nonce, winner: self.with })];
                out.extend(self.over(Some(self.with), now));
                out
            }
            Msg::DuelOver { winner, .. } if !self.referee && !matches!(self.phase, Phase::Over { .. } | Phase::Done) => self.over((winner != 255).then_some(winner), now),
            // Called off by the other before it was decided.
            Msg::Quit { .. } => self.called_off(tr("The duel is called off")),
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    #[test]
    fn room_comes_before_the_pistol() {
        assert_eq!(duel_refusal(false, false), Some("Not enough room to duel here"));
        assert_eq!(duel_refusal(false, true), Some("Not enough room to duel here"));
        assert_eq!(duel_refusal(true, false), Some("Equip a pistol to duel"));
        assert_eq!(duel_refusal(true, true), None);
    }

    #[test]
    fn room_is_an_open_line_outdoors() {
        let open = [1000.0; 8];
        assert!(room_for_duel(false, &open));
        assert!(!room_for_duel(true, &open), "never indoors");
        let mut corridor = [50.0; 8];
        corridor[2] = 400.0;
        corridor[6] = 400.0;
        assert!(room_for_duel(false, &corridor), "a clear line east-west");
        corridor[6] = 200.0;
        assert!(!room_for_duel(false, &corridor), "a dead end one way");
    }

    #[test]
    fn pistols_only() {
        assert!(is_pistol(Some("Pistol - Ballistic (1 Hand)")));
        assert!(is_pistol(Some("Pistol - Energy (1 Hand)")));
        assert!(!is_pistol(Some("Rifle - Ballistic (2 Hand)")));
        assert!(!is_pistol(Some("Melee (1 Hand)")));
        assert!(!is_pistol(None));
    }

    #[test]
    fn stances() {
        let me = [0.0, 0.0, 0.0];
        let north = [0.0, 400.0, 0.0];
        assert_eq!(stance(false, me, PI, north), Stance::NoPistol);
        assert_eq!(stance(true, me, PI, [0.0, 50.0, 0.0]), Stance::TooClose);
        assert_eq!(stance(true, me, PI, [0.0, 5000.0, 0.0]), Stance::TooFar);
        assert_eq!(stance(true, me, 0.0, north), Stance::Facing, "facing the other");
        assert_eq!(stance(true, me, PI / 2.0, north), Stance::Facing, "sideways");
        assert_eq!(stance(true, me, PI, north), Stance::Ready, "back turned");
        assert_eq!(stance(true, me, -PI + 0.3, north), Stance::Ready, "near enough");
        let east = [400.0, 0.0, 0.0];
        assert_eq!(stance(true, me, 3.0 * PI / 2.0, east), Stance::Ready);
        assert_eq!(stance(true, me, PI / 2.0, east), Stance::Facing);
    }

    #[test]
    fn the_countdown_is_two_to_seven_seconds() {
        for r in [0, 1, 4_999, 5_000, 5_001, 123_456_789, u32::MAX] {
            let d = delay_for(r);
            assert!((2_000..=7_000).contains(&d), "{r}: {d}");
        }
        assert_eq!(delay_for(0), 2_000);
        assert_eq!(delay_for(5_000), 7_000);
    }

    #[test]
    fn the_loser_is_thrown_away_from_the_winner() {
        let t = throw([100.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
        assert!(t[0] > 0.0 && t[1].abs() < 1e-3 && t[2] > 0.0);
        assert!(throw([0.0; 3], [0.0; 3])[2] > 0.0, "the same spot still throws");
    }

    /// Pass every Send of `from` to `to`.
    fn pass(outs: &[Out], to: &mut Duel, now: f32) -> Vec<Out> {
        let mut back = Vec::new();
        for o in outs {
            if let Out::Send(m) = o {
                back.extend(to.heard(m, now));
            }
        }
        back
    }

    fn says(outs: &[Out], s: &str) -> bool {
        outs.iter().any(|o| matches!(o, Out::Say(l) if l.contains(s)))
    }

    #[test]
    fn a_whole_duel() {
        let mut a = Duel::new(0, 1, 9, true, 0.0);
        let mut b = Duel::new(1, 0, 9, false, 0.0);
        // Positioning: told to equip, then to turn.
        let oa = a.step(0.0, Stance::NoPistol, "B", 0);
        assert!(says(&oa, "Equip a pistol"));
        pass(&oa, &mut b, 0.0);
        let ob = b.step(0.0, Stance::Facing, "A", 0);
        assert!(says(&ob, "Turn your back to A"));
        pass(&ob, &mut a, 0.0);
        // Both ready: the referee waits SETTLE, then freezes both.
        let ob = b.step(1.0, Stance::Ready, "A", 0);
        pass(&ob, &mut a, 1.0);
        let oa = a.step(1.0, Stance::Ready, "B", 0);
        assert!(!a.frozen());
        pass(&oa, &mut b, 1.0);
        let oa = a.step(1.0 + SETTLE, Stance::Ready, "B", 1_000);
        assert!(a.frozen(), "{:?}", a.phase);
        pass(&oa, &mut b, 1.0 + SETTLE);
        assert!(b.frozen());
        let Phase::Countdown { draw_at } = a.phase else { panic!() };
        assert!((draw_at - (1.0 + SETTLE + 3.0)).abs() < 1e-3, "{draw_at}");
        // Nothing to say during the countdown: it is never shown.
        assert!(a.step(draw_at - 0.01, Stance::Ready, "B", 0).is_empty());
        // The draw.
        assert_eq!(a.step(draw_at, Stance::Ready, "B", 0), vec![Out::Sweep]);
        assert_eq!(b.step(draw_at + 0.05, Stance::Ready, "A", 0), vec![Out::Sweep]);
        assert!(a.drawing() && b.drawing() && !a.frozen());
        // B hits first: the referee says B won; A is thrown, B sees A fall.
        let ob = b.hit(draw_at + 0.4);
        assert_eq!(ob, vec![Out::Send(Msg::DuelHit { nonce: 9 })]);
        assert!(b.hit(draw_at + 0.5).is_empty(), "one hit counts");
        let back = pass(&ob, &mut a, draw_at + 0.45);
        assert!(back.contains(&Out::Thrown) && says(&back, "You lose"));
        // A's own hit after B's does not count.
        assert!(a.hit(draw_at + 0.5).is_empty());
        let verdict = vec![Out::Send(Msg::DuelOver { nonce: 9, winner: 1 })];
        let seen = pass(&verdict, &mut b, draw_at + 0.5);
        assert!(seen.contains(&Out::TheyFall) && says(&seen, "You win"));
        assert!(a.harmless());
        a.step(draw_at + 0.46 + OVER_SECONDS, Stance::Ready, "B", 0);
        assert!(a.done() && !a.harmless());
    }

    #[test]
    fn the_referees_own_hit_wins_and_a_draw_without_hits() {
        let mut a = Duel::new(0, 1, 1, true, 0.0);
        a.heard(&Msg::DuelReady { nonce: 1, ready: true }, 0.0);
        a.step(0.0, Stance::Ready, "B", 0);
        a.step(SETTLE, Stance::Ready, "B", 0);
        let Phase::Countdown { draw_at } = a.phase else { panic!("{:?}", a.phase) };
        assert!(a.hit(draw_at - 0.1).is_empty(), "no shot counts before the signal");
        a.step(draw_at, Stance::Ready, "B", 0);
        let o = a.hit(draw_at + 0.2);
        assert!(o.contains(&Out::Send(Msg::DuelOver { nonce: 1, winner: 0 })) && o.contains(&Out::TheyFall));
        assert!(a.heard(&Msg::DuelHit { nonce: 1 }, draw_at + 0.3).is_empty(), "too late");
        // Nobody hits: a draw.
        let mut c = Duel::new(0, 1, 2, true, 0.0);
        c.heard(&Msg::DuelReady { nonce: 2, ready: true }, 0.0);
        c.step(0.0, Stance::Ready, "B", 0);
        c.step(SETTLE, Stance::Ready, "B", 0);
        let Phase::Countdown { draw_at } = c.phase else { panic!() };
        c.step(draw_at, Stance::Ready, "B", 0);
        let o = c.step(draw_at + DRAW_SECONDS + 0.01, Stance::Ready, "B", 0);
        assert!(o.contains(&Out::Send(Msg::DuelOver { nonce: 2, winner: 255 })) && says(&o, "draw"));
    }

    #[test]
    fn readiness_lost_before_the_countdown_holds_it_back() {
        let mut a = Duel::new(0, 1, 1, true, 0.0);
        a.heard(&Msg::DuelReady { nonce: 1, ready: true }, 0.0);
        a.step(0.0, Stance::Ready, "B", 0);
        a.heard(&Msg::DuelReady { nonce: 1, ready: false }, 0.3);
        a.step(SETTLE, Stance::Ready, "B", 0);
        assert_eq!(a.phase, Phase::Positioning);
        // The other side's countdown is clamped to the range whatever it is told.
        let mut b = Duel::new(1, 0, 1, false, 0.0);
        b.heard(&Msg::DuelFreeze { nonce: 1, delay_ms: 60_000 }, 0.0);
        assert_eq!(b.phase, Phase::Countdown { draw_at: 7.0 });
        // Called off by the other.
        let o = b.heard(&Msg::Quit { nonce: 1 }, 1.0);
        assert!(b.done() && says(&o, "called off"));
        // Nobody in position for too long.
        let mut c = Duel::new(0, 1, 3, true, 0.0);
        let o = c.step(POSITION_SECONDS, Stance::Facing, "B", 0);
        assert!(c.done() && o.contains(&Out::Send(Msg::Quit { nonce: 3 })));
    }

    #[test]
    fn a_duel_ends_when_either_player_goes_and_says_so_once() {
        let mut a = Duel::new(0, 1, 1, true, 0.0);
        let o = a.other_gone();
        assert!(a.done() && says(&o, "called off") && !a.harmless(), "the other left: over, no more spared hits");
        assert!(a.other_gone().is_empty(), "said once");
        let mut b = Duel::new(1, 0, 1, false, 0.0);
        let o = b.this_dropped();
        assert!(b.done() && says(&o, "connection was lost"));
        assert!(b.this_dropped().is_empty());
    }
}
