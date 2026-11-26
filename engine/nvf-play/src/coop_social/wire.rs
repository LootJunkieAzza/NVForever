//! What two players say to each other for the interaction wheel: one [`Msg`] per action
//! of the game [`crate::coop_tables::game::SOCIAL`], its table the player it is for
//! (`nvf_coop::action`: on the reliable stream, through the host, which checks each).
//! Little-endian, a kind byte first; a blob that does not parse is ignored.

use fnv_util::words::tr;
use crate::coop_tables::{R, W};

/// What an invitation asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Duel,
    Trade,
}

impl Kind {
    pub const ALL: [Kind; 2] = [Kind::Duel, Kind::Trade];

    pub fn code(self) -> u8 {
        match self {
            Kind::Duel => 1,
            Kind::Trade => 2,
        }
    }

    pub fn of(c: u8) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.code() == c)
    }

    /// The wheel's word for it.
    pub fn label(self) -> &'static str {
        match self {
            Kind::Duel => tr("Duel"),
            Kind::Trade => tr("Trade"),
        }
    }
}

/// The most stock items one message carries (6 bytes each, under the table blob's limit).
pub const STOCK_PAGE: usize = 120;

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
    /// the same [design: the propulsion is sent to all players].
    Thrown { nonce: u16, velocity: [i32; 3] },
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
}

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
            | Msg::TradeBrowse { nonce }
            | Msg::TradeStock { nonce, .. }
            | Msg::TradeRequest { nonce, .. }
            | Msg::TradePrice { nonce, .. }
            | Msg::TradeBuy { nonce, .. }
            | Msg::TradeSold { nonce, .. }
            | Msg::TradeRefused { nonce, .. } => *nonce,
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
            20 => Msg::TradeBrowse { nonce },
            21 => {
                let (page, pages, n) = (r.u8()?, r.u8()?, r.u8()? as usize);
                if n > STOCK_PAGE || page >= pages {
                    return None;
                }
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
            Msg::Invite { kind: Kind::Trade, nonce: 7 },
            Msg::Answer { nonce: 7, yes: true },
            Msg::Answer { nonce: 7, yes: false },
            Msg::Quit { nonce: 65_535 },
            Msg::DuelReady { nonce: 1, ready: true },
            Msg::DuelFreeze { nonce: 1, delay_ms: 6_999 },
            Msg::DuelHit { nonce: 1 },
            Msg::DuelOver { nonce: 1, winner: 255 },
            Msg::Thrown { nonce: 1, velocity: [-226, 226, 180] },
            Msg::TradeBrowse { nonce: 2 },
            Msg::TradeStock { nonce: 2, page: 0, pages: 2, items: vec![(0x0001_5169, 3), (0xF, 1)] },
            Msg::TradeRequest { nonce: 2, item: 0x0001_5169, count: 2 },
            Msg::TradePrice { nonce: 2, item: 0x0001_5169, each: 45 },
            Msg::TradeBuy { nonce: 2, item: 0x0001_5169, count: 2, each: 45 },
            Msg::TradeSold { nonce: 2, item: 0x0001_5169, count: 2, each: 45 },
            Msg::TradeRefused { nonce: 2, item: 0x0001_5169, count: 2, each: 45 },
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

    /// The co-op wire's limit for an action's blob (`nvf_coop::action::BLOB_MAX`).
    fn nvf_coop_blob_limit() -> usize {
        1024
    }

    #[test]
    fn a_full_stock_page_fits_the_wire() {
        let stock = Msg::TradeStock { nonce: 1, page: 0, pages: 1, items: vec![(1, 1); STOCK_PAGE] };
        assert!(stock.encode().len() <= nvf_coop_blob_limit());
    }

    #[test]
    fn an_oversized_or_misnumbered_stock_page_is_refused() {
        let mut b = Msg::TradeStock { nonce: 1, page: 0, pages: 1, items: vec![(1, 1)] }.encode();
        // The count byte (after kind, nonce, page and pages) says more than a page holds.
        b[5] = (STOCK_PAGE + 1) as u8;
        b.extend(std::iter::repeat_n(0, 6 * STOCK_PAGE));
        assert_eq!(Msg::decode(&b), None, "more items than a page");
        let past = Msg::TradeStock { nonce: 1, page: 2, pages: 2, items: Vec::new() }.encode();
        assert_eq!(Msg::decode(&past), None, "a page past the last");
    }
}
