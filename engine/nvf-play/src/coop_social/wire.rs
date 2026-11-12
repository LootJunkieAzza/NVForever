//! What two players say to each other for the interaction wheel: one [`Msg`] per table
//! action of the game [`crate::coop_tables::game::SOCIAL`], its table the player it is
//! for (`nvf_coop::net`'s table actions go to everyone through the host, three copies,
//! each taken once: no change to the co-op protocol). Little-endian, a kind byte first;
//! a blob that does not parse is ignored.

use crate::coop_tables::{R, W};

/// What an invitation asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Duel,
    Trade,
    Caravan,
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Duel, Kind::Trade, Kind::Caravan];

    pub fn code(self) -> u8 {
        match self {
            Kind::Duel => 1,
            Kind::Trade => 2,
            Kind::Caravan => 3,
        }
    }

    pub fn of(c: u8) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.code() == c)
    }

    /// The wheel's word for it.
    pub fn label(self) -> &'static str {
        match self {
            Kind::Duel => "Duel",
            Kind::Trade => "Trade",
            Kind::Caravan => "Play Caravan",
        }
    }
}

/// A caravan move in the sender's own frame (its side 0, the other's 1): the receiver
/// turns `side` round ([`CaravanMove::flipped`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaravanMove {
    /// 0 play, 1 discard, 2 discard track.
    pub kind: u8,
    pub card: u8,
    pub side: u8,
    pub track: u8,
    /// 255: none.
    pub on: u8,
}

impl CaravanMove {
    pub fn of(m: fnv_game::caravan::Move) -> CaravanMove {
        use fnv_game::caravan::Move;
        match m {
            Move::Play { card, side, track, on } => CaravanMove { kind: 0, card: card as u8, side: side as u8, track: track as u8, on: on.map_or(255, |i| i as u8) },
            Move::Discard { card } => CaravanMove { kind: 1, card: card as u8, side: 0, track: 0, on: 255 },
            Move::DiscardTrack { track } => CaravanMove { kind: 2, card: 0, side: 0, track: track as u8, on: 255 },
        }
    }

    /// The move as the receiving game sees it: the sides swapped.
    pub fn flipped(self) -> Option<fnv_game::caravan::Move> {
        use fnv_game::caravan::Move;
        match self.kind {
            0 if self.side < 2 => Some(Move::Play { card: self.card as usize, side: 1 - self.side as usize, track: self.track as usize, on: (self.on != 255).then_some(self.on as usize) }),
            1 => Some(Move::Discard { card: self.card as usize }),
            2 => Some(Move::DiscardTrack { track: self.track as usize }),
            _ => None,
        }
    }
}

/// The most stock items one message carries (8 bytes each, under the table blob's limit).
pub const STOCK_PAGE: usize = 120;
/// The most caravan cards one deck message carries.
pub const DECK_MAX: usize = 250;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Msg {
    /// Would you like to …? `nonce` names the session from here on.
    Invite { kind: Kind, nonce: u16 },
    Answer { nonce: u16, yes: bool },
    /// The session is over for the sender (it left, cancelled, went away).
    Quit { nonce: u16 },
    /// Duel: the sender is in position (back turned, far enough, a pistol), or not.
    DuelReady { nonce: u16, ready: bool },
    /// Duel, from the challenger: freeze now; the draw comes after `delay_ms`.
    DuelFreeze { nonce: u16, delay_ms: u16 },
    /// Duel: the sender's shot hit.
    DuelHit { nonce: u16 },
    /// Duel, from the challenger: who won (a player id; 255 nobody).
    DuelOver { nonce: u16, winner: u8 },
    /// Duel, from the loser to every player: its own game threw its body with this
    /// velocity (whole game units a second, game space), so every copy of it is thrown
    /// the same (owner 2026-11-09: "the propulsion should be sent to all players").
    Thrown { nonce: u16, velocity: [i32; 3] },
    /// To every player: the sender's game set off this explosion (EXPL) at `point` (whole
    /// game units), so every game draws and hears it (playtest 119 item 162).
    Blast { nonce: u16, explosion: u32, point: [i32; 3] },
    /// Trade: send me what you carry.
    TradeBrowse { nonce: u16 },
    /// Trade: what the sender carries (item, count), page `page` of `pages`.
    TradeStock { nonce: u16, page: u8, pages: u8, items: Vec<(u32, u16)> },
    /// Trade: the sender would like `count` of `item`.
    TradeRequest { nonce: u16, item: u32, count: u16 },
    /// Trade: the owner's price for each of `item` asked for (0: it will not sell).
    TradePrice { nonce: u16, item: u32, each: u32 },
    /// Trade: the requester pays `each` × `count` caps for `item` (its caps are held).
    TradeBuy { nonce: u16, item: u32, count: u16, each: u32 },
    /// Trade: the owner gave the items and took the caps.
    TradeSold { nonce: u16, item: u32, count: u16, each: u32 },
    /// Trade: the owner could not sell (it no longer has them, the price changed).
    TradeRefused { nonce: u16, item: u32, count: u16, each: u32 },
    /// Caravan: the sender's deck (card FormIDs).
    CaravanDeck { nonce: u16, cards: Vec<u32> },
    CaravanMove { nonce: u16, mv: CaravanMove },
    /// Caravan: the sender's caps, and its offered stake ([`NO_OFFER`]: none yet);
    /// `agreed`: the leader (the inviter) announces the stake (`crate::stake`).
    CaravanBet { nonce: u16, funds: u32, ante: u32, agreed: bool },
    /// A duel's stake, as [`Msg::CaravanBet`] (`crate::stake`).
    DuelBet { nonce: u16, funds: u32, ante: u32, agreed: bool },
}

/// [`Msg::CaravanBet`]'s `ante` when the sender only says its caps.
pub const NO_OFFER: u32 = u32::MAX;

impl Msg {
    pub fn nonce(&self) -> u16 {
        match self {
            Msg::Invite { nonce, .. }
            | Msg::Answer { nonce, .. }
            | Msg::Quit { nonce }
            | Msg::DuelReady { nonce, .. }
            | Msg::DuelFreeze { nonce, .. }
            | Msg::DuelHit { nonce }
            | Msg::DuelOver { nonce, .. }
            | Msg::Thrown { nonce, .. }
            | Msg::Blast { nonce, .. }
            | Msg::TradeBrowse { nonce }
            | Msg::TradeStock { nonce, .. }
            | Msg::TradeRequest { nonce, .. }
            | Msg::TradePrice { nonce, .. }
            | Msg::TradeBuy { nonce, .. }
            | Msg::TradeSold { nonce, .. }
            | Msg::TradeRefused { nonce, .. }
            | Msg::CaravanDeck { nonce, .. }
            | Msg::CaravanMove { nonce, .. }
            | Msg::CaravanBet { nonce, .. }
            | Msg::DuelBet { nonce, .. } => *nonce,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::default();
        match self {
            Msg::Invite { kind, nonce } => w.u8(1).u16(*nonce).u8(kind.code()),
            Msg::Answer { nonce, yes } => w.u8(2).u16(*nonce).u8(*yes as u8),
            Msg::Quit { nonce } => w.u8(3).u16(*nonce),
            Msg::DuelReady { nonce, ready } => w.u8(10).u16(*nonce).u8(*ready as u8),
            Msg::DuelFreeze { nonce, delay_ms } => w.u8(11).u16(*nonce).u16(*delay_ms),
            Msg::DuelHit { nonce } => w.u8(12).u16(*nonce),
            Msg::DuelOver { nonce, winner } => w.u8(13).u16(*nonce).u8(*winner),
            Msg::Thrown { nonce, velocity } => w.u8(14).u16(*nonce).u32(velocity[0] as u32).u32(velocity[1] as u32).u32(velocity[2] as u32),
            Msg::Blast { nonce, explosion, point } => w.u8(15).u16(*nonce).u32(*explosion).u32(point[0] as u32).u32(point[1] as u32).u32(point[2] as u32),
            Msg::TradeBrowse { nonce } => w.u8(20).u16(*nonce),
            Msg::TradeStock { nonce, page, pages, items } => {
                w.u8(21).u16(*nonce).u8(*page).u8(*pages).u8(items.len().min(STOCK_PAGE) as u8);
                for (f, n) in items.iter().take(STOCK_PAGE) {
                    w.u32(*f).u16(*n);
                }
                &mut w
            }
            Msg::TradeRequest { nonce, item, count } => w.u8(22).u16(*nonce).u32(*item).u16(*count),
            Msg::TradePrice { nonce, item, each } => w.u8(23).u16(*nonce).u32(*item).u32(*each),
            Msg::TradeBuy { nonce, item, count, each } => w.u8(24).u16(*nonce).u32(*item).u16(*count).u32(*each),
            Msg::TradeSold { nonce, item, count, each } => w.u8(25).u16(*nonce).u32(*item).u16(*count).u32(*each),
            Msg::TradeRefused { nonce, item, count, each } => w.u8(26).u16(*nonce).u32(*item).u16(*count).u32(*each),
            Msg::CaravanDeck { nonce, cards } => {
                w.u8(30).u16(*nonce).u8(cards.len().min(DECK_MAX) as u8);
                for c in cards.iter().take(DECK_MAX) {
                    w.u32(*c);
                }
                &mut w
            }
            Msg::CaravanMove { nonce, mv } => w.u8(31).u16(*nonce).u8(mv.kind).u8(mv.card).u8(mv.side).u8(mv.track).u8(mv.on),
            Msg::CaravanBet { nonce, funds, ante, agreed } => w.u8(32).u16(*nonce).u32(*funds).u32(*ante).u8(*agreed as u8),
            Msg::DuelBet { nonce, funds, ante, agreed } => w.u8(33).u16(*nonce).u32(*funds).u32(*ante).u8(*agreed as u8),
        };
        w.0
    }

    pub fn decode(b: &[u8]) -> Option<Msg> {
        let mut r = R(b);
        let kind = r.u8()?;
        let nonce = r.u16()?;
        let m = match kind {
            1 => Msg::Invite { nonce, kind: Kind::of(r.u8()?)? },
            2 => Msg::Answer { nonce, yes: r.u8()? != 0 },
            3 => Msg::Quit { nonce },
            10 => Msg::DuelReady { nonce, ready: r.u8()? != 0 },
            11 => Msg::DuelFreeze { nonce, delay_ms: r.u16()? },
            12 => Msg::DuelHit { nonce },
            13 => Msg::DuelOver { nonce, winner: r.u8()? },
            14 => Msg::Thrown { nonce, velocity: [r.u32()? as i32, r.u32()? as i32, r.u32()? as i32] },
            15 => Msg::Blast { nonce, explosion: r.u32()?, point: [r.u32()? as i32, r.u32()? as i32, r.u32()? as i32] },
            20 => Msg::TradeBrowse { nonce },
            21 => {
                let (page, pages, n) = (r.u8()?, r.u8()?, r.u8()? as usize);
                let mut items = Vec::with_capacity(n);
                for _ in 0..n {
                    items.push((r.u32()?, r.u16()?));
                }
                Msg::TradeStock { nonce, page, pages, items }
            }
            22 => Msg::TradeRequest { nonce, item: r.u32()?, count: r.u16()? },
            23 => Msg::TradePrice { nonce, item: r.u32()?, each: r.u32()? },
            24 => Msg::TradeBuy { nonce, item: r.u32()?, count: r.u16()?, each: r.u32()? },
            25 => Msg::TradeSold { nonce, item: r.u32()?, count: r.u16()?, each: r.u32()? },
            26 => Msg::TradeRefused { nonce, item: r.u32()?, count: r.u16()?, each: r.u32()? },
            30 => {
                let n = r.u8()? as usize;
                let mut cards = Vec::with_capacity(n);
                for _ in 0..n {
                    cards.push(r.u32()?);
                }
                Msg::CaravanDeck { nonce, cards }
            }
            31 => Msg::CaravanMove { nonce, mv: CaravanMove { kind: r.u8()?, card: r.u8()?, side: r.u8()?, track: r.u8()?, on: r.u8()? } },
            32 => Msg::CaravanBet { nonce, funds: r.u32()?, ante: r.u32()?, agreed: r.u8()? != 0 },
            33 => Msg::DuelBet { nonce, funds: r.u32()?, ante: r.u32()?, agreed: r.u8()? != 0 },
            _ => return None,
        };
        r.done().then_some(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_message_comes_back_whole() {
        let all = [
            Msg::Invite { kind: Kind::Caravan, nonce: 7 },
            Msg::Answer { nonce: 7, yes: true },
            Msg::Answer { nonce: 7, yes: false },
            Msg::Quit { nonce: 65_535 },
            Msg::DuelReady { nonce: 1, ready: true },
            Msg::DuelFreeze { nonce: 1, delay_ms: 6_999 },
            Msg::DuelHit { nonce: 1 },
            Msg::DuelOver { nonce: 1, winner: 255 },
            Msg::Thrown { nonce: 1, velocity: [-226, 226, 180] },
            Msg::Blast { nonce: 0, explosion: 0x0010_E2A0, point: [-69_000, 2_446, -8_700] },
            Msg::TradeBrowse { nonce: 2 },
            Msg::TradeStock { nonce: 2, page: 0, pages: 2, items: vec![(0x0001_5169, 3), (0xF, 1)] },
            Msg::TradeRequest { nonce: 2, item: 0x0001_5169, count: 2 },
            Msg::TradePrice { nonce: 2, item: 0x0001_5169, each: 45 },
            Msg::TradeBuy { nonce: 2, item: 0x0001_5169, count: 2, each: 45 },
            Msg::TradeSold { nonce: 2, item: 0x0001_5169, count: 2, each: 45 },
            Msg::TradeRefused { nonce: 2, item: 0x0001_5169, count: 2, each: 45 },
            Msg::CaravanDeck { nonce: 3, cards: (0..40).map(|i| 0x0010_0000 + i).collect() },
            Msg::CaravanMove { nonce: 3, mv: CaravanMove { kind: 0, card: 4, side: 1, track: 2, on: 255 } },
            Msg::CaravanBet { nonce: 3, funds: 1_250, ante: 60, agreed: true },
            Msg::CaravanBet { nonce: 3, funds: 0, ante: NO_OFFER, agreed: false },
            Msg::DuelBet { nonce: 4, funds: 300, ante: 30, agreed: false },
        ];
        for m in all {
            let b = m.encode();
            assert!(b.len() <= nvf_coop_blob_limit(), "{m:?} is too long");
            assert_eq!(Msg::decode(&b), Some(m.clone()), "{m:?}");
            assert_eq!(Msg::decode(&b[..b.len() - 1]), None, "{m:?} cut short");
        }
        assert_eq!(Msg::decode(&[99, 0, 0]), None);
        assert_eq!(Msg::decode(&[1, 0, 0, 9]), None, "an unknown kind of invitation");
    }

    /// The co-op wire's limit for a table blob (`nvf_coop::net::TABLE_MAX`).
    fn nvf_coop_blob_limit() -> usize {
        1100
    }

    #[test]
    fn a_full_stock_page_and_deck_fit_the_wire() {
        let stock = Msg::TradeStock { nonce: 1, page: 0, pages: 1, items: vec![(1, 1); STOCK_PAGE] };
        assert!(stock.encode().len() <= nvf_coop_blob_limit());
        let deck = Msg::CaravanDeck { nonce: 1, cards: vec![1; DECK_MAX] };
        assert!(deck.encode().len() <= nvf_coop_blob_limit());
    }

    #[test]
    fn a_caravan_move_is_seen_from_the_other_side() {
        use fnv_game::caravan::Move;
        let mine = Move::Play { card: 2, side: 0, track: 1, on: None };
        assert_eq!(CaravanMove::of(mine).flipped(), Some(Move::Play { card: 2, side: 1, track: 1, on: None }));
        let theirs = Move::Play { card: 0, side: 1, track: 0, on: Some(3) };
        assert_eq!(CaravanMove::of(theirs).flipped(), Some(Move::Play { card: 0, side: 0, track: 0, on: Some(3) }));
        assert_eq!(CaravanMove::of(Move::Discard { card: 5 }).flipped(), Some(Move::Discard { card: 5 }));
        assert_eq!(CaravanMove::of(Move::DiscardTrack { track: 2 }).flipped(), Some(Move::DiscardTrack { track: 2 }));
        assert_eq!(CaravanMove { kind: 9, card: 0, side: 0, track: 0, on: 255 }.flipped(), None);
    }
}
