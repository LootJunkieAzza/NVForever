//! Invitations: one out at a time, the others' shown one at a time as a Y/N prompt naming
//! the sender. An invitation not answered in [`INVITE_SECONDS`] lapses on both sides; a
//! player already busy (in a duel, a trade, a game) declines by itself.

use super::wire::{Kind, Msg};
use std::collections::VecDeque;

/// Real seconds an invitation waits for its answer [design].
pub const INVITE_SECONDS: f32 = 20.0;

/// The prompt's line: the sender's real name (the owner's words, 2026-11-04).
pub fn invite_line(name: &str, kind: Kind) -> String {
    match kind {
        Kind::Duel => format!("{name} would like to duel"),
        Kind::Trade => format!("{name} would like to trade"),
        Kind::Caravan => format!("{name} would like to play caravan"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Invite {
    /// The other player.
    pub with: u8,
    pub kind: Kind,
    pub nonce: u16,
    /// When it was sent or heard (real seconds).
    pub since: f32,
}

/// What the invitations came to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Settled {
    /// Both said yes: the session starts. `inviter`: this player asked.
    Accepted { with: u8, kind: Kind, nonce: u16, inviter: bool },
    /// The other said no (or was busy).
    Declined { with: u8, kind: Kind },
    /// Nobody answered in time: this player's own (`mine`) or the other's.
    Lapsed { with: u8, kind: Kind, mine: bool },
    /// The other took its invitation back (it left, or asked something else).
    Withdrawn { with: u8, kind: Kind },
}

#[derive(Debug, Default)]
pub struct Invites {
    /// This player's invitation, waiting for its answer.
    pub out: Option<Invite>,
    /// The others' invitations, the first shown.
    pub asked: VecDeque<Invite>,
    count: u16,
}

impl Invites {
    /// A nonce no other player makes: the player's id in the top bits.
    fn nonce(&mut self, me: u8) -> u16 {
        self.count = (self.count + 1) & 0x0FFF;
        ((me as u16 & 0xF) << 12) | self.count
    }

    /// Ask `to` for `kind`. Refused while an invitation of this player's waits.
    pub fn send(&mut self, me: u8, to: u8, kind: Kind, now: f32, out: &mut Vec<(u8, Msg)>) -> Result<u16, &'static str> {
        if self.out.is_some() {
            return Err("waiting for an answer");
        }
        let nonce = self.nonce(me);
        self.out = Some(Invite { with: to, kind, nonce, since: now });
        out.push((to, Msg::Invite { kind, nonce }));
        Ok(nonce)
    }

    /// The prompt shown now.
    pub fn shown(&self) -> Option<&Invite> {
        self.asked.front()
    }

    /// An invitation heard from `from`. A busy player declines at once; one already
    /// heard (a copy) is kept once. Asked by the player this one is asking (`me`), for
    /// the same thing: both want it, and the lower id's invitation stands on both sides
    /// (the higher id accepts it and takes its own back; the lower id waits for that yes).
    #[allow(clippy::too_many_arguments)]
    pub fn heard(&mut self, me: u8, from: u8, kind: Kind, nonce: u16, now: f32, busy: bool, out: &mut Vec<(u8, Msg)>) -> Option<Settled> {
        if self.asked.iter().any(|i| i.nonce == nonce && i.with == from) {
            return None;
        }
        if busy {
            out.push((from, Msg::Answer { nonce, yes: false }));
            return None;
        }
        if let Some(mine) = self.out.filter(|o| o.with == from && o.kind == kind) {
            if me < from {
                return None;
            }
            self.out = None;
            out.push((from, Msg::Quit { nonce: mine.nonce }));
            out.push((from, Msg::Answer { nonce, yes: true }));
            return Some(Settled::Accepted { with: from, kind, nonce, inviter: false });
        }
        // A newer invitation from the same player takes the old one's place.
        self.asked.retain(|i| i.with != from);
        self.asked.push_back(Invite { with: from, kind, nonce, since: now });
        None
    }

    /// The shown invitation answered by this player.
    pub fn answer(&mut self, yes: bool, out: &mut Vec<(u8, Msg)>) -> Option<Settled> {
        let i = self.asked.pop_front()?;
        out.push((i.with, Msg::Answer { nonce: i.nonce, yes }));
        if yes {
            // Every other one is declined: this player is busy now.
            for o in self.asked.drain(..) {
                out.push((o.with, Msg::Answer { nonce: o.nonce, yes: false }));
            }
            if let Some(mine) = self.out.take() {
                out.push((mine.with, Msg::Quit { nonce: mine.nonce }));
            }
            Some(Settled::Accepted { with: i.with, kind: i.kind, nonce: i.nonce, inviter: false })
        } else {
            None
        }
    }

    /// The answer to this player's invitation.
    pub fn answered(&mut self, from: u8, nonce: u16, yes: bool) -> Option<Settled> {
        let o = self.out.filter(|o| o.with == from && o.nonce == nonce)?;
        self.out = None;
        Some(if yes { Settled::Accepted { with: from, kind: o.kind, nonce, inviter: true } } else { Settled::Declined { with: from, kind: o.kind } })
    }

    /// `from` took invitation `nonce` back.
    pub fn withdrawn(&mut self, from: u8, nonce: u16) -> Option<Settled> {
        let i = self.asked.iter().position(|i| i.with == from && i.nonce == nonce)?;
        let i = self.asked.remove(i)?;
        Some(Settled::Withdrawn { with: i.with, kind: i.kind })
    }

    /// Take this player's invitation back.
    pub fn cancel(&mut self, out: &mut Vec<(u8, Msg)>) {
        if let Some(o) = self.out.take() {
            out.push((o.with, Msg::Quit { nonce: o.nonce }));
        }
    }

    /// A player who left: its invitations go, and this player's to it.
    pub fn gone(&mut self, player: u8) {
        self.asked.retain(|i| i.with != player);
        if self.out.is_some_and(|o| o.with == player) {
            self.out = None;
        }
    }

    /// The invitations that waited too long.
    pub fn step(&mut self, now: f32, out: &mut Vec<(u8, Msg)>) -> Vec<Settled> {
        let mut done = Vec::new();
        if let Some(o) = self.out.filter(|o| now - o.since >= INVITE_SECONDS) {
            self.out = None;
            out.push((o.with, Msg::Quit { nonce: o.nonce }));
            done.push(Settled::Lapsed { with: o.with, kind: o.kind, mine: true });
        }
        while let Some(i) = self.asked.front().copied().filter(|i| now - i.since >= INVITE_SECONDS) {
            self.asked.pop_front();
            out.push((i.with, Msg::Answer { nonce: i.nonce, yes: false }));
            done.push(Settled::Lapsed { with: i.with, kind: i.kind, mine: false });
        }
        done
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prompt_names_the_sender() {
        assert_eq!(invite_line("Courier Six", Kind::Duel), "Courier Six would like to duel");
        assert_eq!(invite_line("Benny", Kind::Trade), "Benny would like to trade");
        assert_eq!(invite_line("Ana", Kind::Caravan), "Ana would like to play caravan");
    }

    /// Two players' invitations, their messages passed by hand.
    fn deliver(from: u8, msgs: Vec<(u8, Msg)>, to: &mut Invites, now: f32, busy: bool) -> (Vec<Settled>, Vec<(u8, Msg)>) {
        let me = msgs.first().map_or(0, |m| m.0);
        let (mut settled, mut back) = (Vec::new(), Vec::new());
        for (_, m) in msgs {
            let s = match m {
                Msg::Invite { kind, nonce } => to.heard(me, from, kind, nonce, now, busy, &mut back),
                Msg::Answer { nonce, yes } => to.answered(from, nonce, yes),
                Msg::Quit { nonce } => to.withdrawn(from, nonce),
                _ => None,
            };
            settled.extend(s);
        }
        (settled, back)
    }

    #[test]
    fn asked_and_accepted() {
        let (mut a, mut b) = (Invites::default(), Invites::default());
        let mut out = Vec::new();
        let nonce = a.send(0, 1, Kind::Duel, 0.0, &mut out).unwrap();
        assert_eq!(nonce >> 12, 0);
        assert!(a.send(0, 1, Kind::Trade, 0.5, &mut Vec::new()).is_err(), "one invitation out at a time");
        let (s, back) = deliver(0, out, &mut b, 0.1, false);
        assert!(s.is_empty() && back.is_empty());
        assert_eq!(b.shown().map(|i| (i.with, i.kind)), Some((0, Kind::Duel)));
        let mut out = Vec::new();
        assert_eq!(b.answer(true, &mut out), Some(Settled::Accepted { with: 0, kind: Kind::Duel, nonce, inviter: false }));
        let (s, _) = deliver(1, out, &mut a, 0.2, false);
        assert_eq!(s, vec![Settled::Accepted { with: 1, kind: Kind::Duel, nonce, inviter: true }]);
        assert!(a.out.is_none() && b.shown().is_none());
    }

    #[test]
    fn declined_busy_and_copies() {
        let (mut a, mut b) = (Invites::default(), Invites::default());
        let mut out = Vec::new();
        a.send(2, 1, Kind::Trade, 0.0, &mut out).unwrap();
        // Three copies come: one prompt.
        let copies: Vec<(u8, Msg)> = out.iter().cloned().chain(out.iter().cloned()).chain(out.iter().cloned()).collect();
        deliver(2, copies, &mut b, 0.1, false);
        assert_eq!(b.asked.len(), 1);
        let mut back = Vec::new();
        assert_eq!(b.answer(false, &mut back), None);
        let (s, _) = deliver(1, back, &mut a, 0.2, false);
        assert_eq!(s, vec![Settled::Declined { with: 1, kind: Kind::Trade }]);
        // Busy: declined by itself, no prompt.
        let mut out = Vec::new();
        a.send(2, 1, Kind::Caravan, 1.0, &mut out).unwrap();
        let (_, back) = deliver(2, out, &mut b, 1.1, true);
        assert!(b.shown().is_none());
        let (s, _) = deliver(1, back, &mut a, 1.2, false);
        assert_eq!(s, vec![Settled::Declined { with: 1, kind: Kind::Caravan }]);
    }

    #[test]
    fn unanswered_invitations_lapse_on_both_sides() {
        let (mut a, mut b) = (Invites::default(), Invites::default());
        let mut out = Vec::new();
        a.send(0, 1, Kind::Duel, 0.0, &mut out).unwrap();
        deliver(0, out, &mut b, 0.0, false);
        assert!(a.step(INVITE_SECONDS - 0.1, &mut Vec::new()).is_empty());
        let mut out = Vec::new();
        assert_eq!(a.step(INVITE_SECONDS, &mut out), vec![Settled::Lapsed { with: 1, kind: Kind::Duel, mine: true }]);
        // Its withdrawal takes the prompt down before it lapses there.
        let (s, _) = deliver(0, out, &mut b, INVITE_SECONDS, false);
        assert_eq!(s, vec![Settled::Withdrawn { with: 0, kind: Kind::Duel }]);
        assert!(b.shown().is_none());
        // A prompt nobody answers lapses too, declined.
        let mut out = Vec::new();
        a.send(0, 1, Kind::Trade, 30.0, &mut out).unwrap();
        deliver(0, out, &mut b, 30.0, false);
        let mut back = Vec::new();
        assert_eq!(b.step(30.0 + INVITE_SECONDS, &mut back), vec![Settled::Lapsed { with: 0, kind: Kind::Trade, mine: false }]);
        assert!(matches!(back[..], [(0, Msg::Answer { yes: false, .. })]));
    }

    #[test]
    fn crossed_invitations_meet() {
        let (mut a, mut b) = (Invites::default(), Invites::default());
        let (mut oa, mut ob) = (Vec::new(), Vec::new());
        let na = a.send(0, 1, Kind::Caravan, 0.0, &mut oa).unwrap();
        b.send(1, 0, Kind::Caravan, 0.0, &mut ob).unwrap();
        // A (the lower id) keeps its own and ignores B's.
        let (sa, back) = deliver(1, ob, &mut a, 0.1, false);
        assert!(sa.is_empty() && back.is_empty() && a.out.is_some());
        // B accepts A's and takes its own back.
        let (sb, back) = deliver(0, oa, &mut b, 0.1, false);
        assert_eq!(sb, vec![Settled::Accepted { with: 0, kind: Kind::Caravan, nonce: na, inviter: false }]);
        assert!(b.out.is_none() && b.shown().is_none());
        let (sa, _) = deliver(1, back, &mut a, 0.2, false);
        assert_eq!(sa, vec![Settled::Accepted { with: 1, kind: Kind::Caravan, nonce: na, inviter: true }]);
    }

    #[test]
    fn accepting_one_declines_the_others() {
        let mut b = Invites::default();
        let mut sink = Vec::new();
        b.heard(1, 0, Kind::Duel, 1, 0.0, false, &mut sink);
        b.heard(1, 2, Kind::Trade, 2 << 12 | 1, 0.0, false, &mut sink);
        let mut out = Vec::new();
        assert!(matches!(b.answer(true, &mut out), Some(Settled::Accepted { with: 0, .. })));
        assert!(out.contains(&(2, Msg::Answer { nonce: 2 << 12 | 1, yes: false })));
        assert!(b.asked.is_empty());
    }
}
