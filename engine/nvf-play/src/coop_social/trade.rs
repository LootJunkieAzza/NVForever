//! A trade between two players, as Fallout 76's player vending works (the owner's spec,
//! 2026-11-04): each browses what the other carries and asks for items; the owner puts a
//! price on each item asked for (a price for one, for stacks); once priced, the asker can
//! buy for caps.
//!
//! Who holds what: the buyer's caps are taken when it buys ([`Trade::buy`]) and given back
//! if the owner cannot sell ([`Trade::refused`]); the owner gives the items and takes the
//! caps only when it agrees ([`Trade::buy_heard`]), and the buyer gets the items when told
//! so ([`Trade::sold`]). Plain state; `super` does the inventory and the wire.

use super::wire::{Msg, STOCK_PAGE};

/// The most a price may be, caps for one [design].
pub const PRICE_MAX: u32 = 99_999;

/// Where one of this player's asks stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AskState {
    /// Waiting for the owner's price.
    Waiting,
    /// Priced: it can be bought.
    Priced(u32),
    /// The owner will not sell it.
    Declined,
    /// Bought at this price for one, the caps held; waiting for the items.
    Buying(u32),
    /// Done: the items came.
    Bought,
}

/// An item this player asked the other for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ask {
    pub item: u32,
    pub count: u16,
    pub state: AskState,
}

/// An item the other asked this player for: the price being set, and the one sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Offer {
    pub item: u32,
    pub count: u16,
    pub each: u32,
    pub sent: Option<u32>,
}

/// What the owner's game is to do with a buy it heard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sale {
    /// Take `count` of `item` from this player, give it `count × each` caps.
    Sell { item: u32, count: u16, each: u32 },
    Refuse,
}

#[derive(Debug, Clone, Default)]
pub struct Trade {
    pub with: u8,
    pub nonce: u16,
    /// What the other carries, as last heard (item, count), and the pages coming in.
    pub stock: Vec<(u32, u16)>,
    pages: Vec<Option<Vec<(u32, u16)>>>,
    pub asks: Vec<Ask>,
    pub offers: Vec<Offer>,
    /// The last stock sent, so it is sent again only when it changes.
    sent_stock: Option<Vec<(u32, u16)>>,
}

impl Trade {
    pub fn new(with: u8, nonce: u16) -> Trade {
        Trade { with, nonce, ..Default::default() }
    }

    /// Ask for the other's stock (on opening).
    pub fn browse(&self) -> Msg {
        Msg::TradeBrowse { nonce: self.nonce }
    }

    /// This player's stock for the other, in pages; empty when unchanged unless `again`.
    pub fn stock_msgs(&mut self, mine: &[(u32, u16)], again: bool) -> Vec<Msg> {
        if !again && self.sent_stock.as_deref() == Some(mine) {
            return Vec::new();
        }
        self.sent_stock = Some(mine.to_vec());
        let chunks: Vec<&[(u32, u16)]> = if mine.is_empty() { vec![&[]] } else { mine.chunks(STOCK_PAGE).collect() };
        let pages = chunks.len().min(255) as u8;
        chunks.into_iter().take(255).enumerate().map(|(i, c)| Msg::TradeStock { nonce: self.nonce, page: i as u8, pages, items: c.to_vec() }).collect()
    }

    /// A page of the other's stock; the stock is replaced once every page has come.
    pub fn stock_heard(&mut self, page: u8, pages: u8, items: Vec<(u32, u16)>) {
        if pages == 0 || page >= pages {
            return;
        }
        if self.pages.len() != pages as usize {
            self.pages = vec![None; pages as usize];
        }
        self.pages[page as usize] = Some(items);
        if self.pages.iter().all(Option::is_some) {
            self.stock = std::mem::take(&mut self.pages).into_iter().flatten().flatten().collect();
        }
    }

    /// How many of `item` the other has, as last heard.
    pub fn in_stock(&self, item: u32) -> u16 {
        self.stock.iter().filter(|(f, _)| *f == item).map(|(_, n)| *n).sum()
    }

    /// Ask for `count` of `item`: a new ask, or a changed one (back to waiting).
    pub fn request(&mut self, item: u32, count: u16) -> Result<Msg, &'static str> {
        let have = self.in_stock(item);
        if count == 0 || count > have {
            return Err("they do not have that many");
        }
        match self.asks.iter_mut().find(|a| a.item == item) {
            Some(a) if matches!(a.state, AskState::Buying(_)) => return Err("already being bought"),
            Some(a) => *a = Ask { item, count, state: AskState::Waiting },
            None => self.asks.push(Ask { item, count, state: AskState::Waiting }),
        }
        Ok(Msg::TradeRequest { nonce: self.nonce, item, count })
    }

    /// The other asked for `count` of `item`; this player has `have`; `value` is the
    /// item's own worth, the price it starts at.
    pub fn request_heard(&mut self, item: u32, count: u16, have: u16, value: u32) {
        let count = count.min(have);
        if count == 0 {
            self.offers.retain(|o| o.item != item);
            return;
        }
        match self.offers.iter_mut().find(|o| o.item == item) {
            Some(o) => {
                o.count = count;
                o.sent = None;
            }
            None => self.offers.push(Offer { item, count, each: value.clamp(1, PRICE_MAX), sent: None }),
        }
    }

    /// Change the price being set for offer `i` by `by` caps.
    pub fn nudge(&mut self, i: usize, by: i64) {
        if let Some(o) = self.offers.get_mut(i) {
            o.each = (o.each as i64 + by).clamp(1, PRICE_MAX as i64) as u32;
        }
    }

    /// Set the price being set for offer `i` (within 1 and [`PRICE_MAX`]).
    pub fn set_price(&mut self, i: usize, each: u32) {
        if let Some(o) = self.offers.get_mut(i) {
            o.each = each.clamp(1, PRICE_MAX);
        }
    }

    /// Send offer `i`'s price.
    pub fn price(&mut self, i: usize) -> Option<Msg> {
        let o = self.offers.get_mut(i)?;
        o.sent = Some(o.each);
        Some(Msg::TradePrice { nonce: self.nonce, item: o.item, each: o.each })
    }

    /// Will not sell offer `i`.
    pub fn decline(&mut self, i: usize) -> Option<Msg> {
        let o = self.offers.remove(i);
        Some(Msg::TradePrice { nonce: self.nonce, item: o.item, each: 0 })
    }

    /// The owner's price for `item` (0: declined).
    pub fn price_heard(&mut self, item: u32, each: u32) {
        if let Some(a) = self.asks.iter_mut().find(|a| a.item == item && !matches!(a.state, AskState::Buying(_) | AskState::Bought)) {
            a.state = if each == 0 { AskState::Declined } else { AskState::Priced(each.min(PRICE_MAX)) };
        }
    }

    /// What ask `i` costs in all, if priced.
    pub fn total(&self, i: usize) -> Option<u32> {
        let a = self.asks.get(i)?;
        match a.state {
            AskState::Priced(each) => Some(each.saturating_mul(a.count as u32)),
            _ => None,
        }
    }

    /// Buy ask `i` with `caps` carried: the caps to take now (held until the owner
    /// answers) and the message.
    pub fn buy(&mut self, i: usize, caps: u32) -> Result<(u32, Msg), &'static str> {
        let total = self.total(i).ok_or("it has no price yet")?;
        if caps < total {
            return Err("not enough caps");
        }
        let a = &mut self.asks[i];
        let AskState::Priced(each) = a.state else { unreachable!() };
        a.state = AskState::Buying(each);
        Ok((total, Msg::TradeBuy { nonce: self.nonce, item: a.item, count: a.count, each }))
    }

    /// The other buys: sold if this player sent that price for at least that many and
    /// still has them (`have`).
    pub fn buy_heard(&mut self, item: u32, count: u16, each: u32, have: u16) -> (Sale, Msg) {
        let ok = self.offers.iter().position(|o| o.item == item && o.sent == Some(each) && count <= o.count && count <= have && count > 0);
        match ok {
            Some(i) => {
                self.offers.remove(i);
                (Sale::Sell { item, count, each }, Msg::TradeSold { nonce: self.nonce, item, count, each })
            }
            None => (Sale::Refuse, Msg::TradeRefused { nonce: self.nonce, item, count, each }),
        }
    }

    /// The owner sold: `count` of `item` are this player's now (once).
    pub fn sold(&mut self, item: u32, count: u16, each: u32) -> bool {
        match self.asks.iter_mut().find(|a| a.item == item && a.count == count && a.state == AskState::Buying(each)) {
            Some(a) => {
                a.state = AskState::Bought;
                true
            }
            None => false,
        }
    }

    /// The owner refused: the caps to give back (once).
    pub fn refused(&mut self, item: u32, count: u16, each: u32) -> Option<u32> {
        let a = self.asks.iter_mut().find(|a| a.item == item && a.count == count && a.state == AskState::Buying(each))?;
        a.state = AskState::Declined;
        Some(each.saturating_mul(count as u32))
    }

    /// The caps held for buys not yet answered (given back if the trade ends without
    /// their answers).
    pub fn held(&self) -> u32 {
        self.asks.iter().map(|a| if let AskState::Buying(each) = a.state { each.saturating_mul(a.count as u32) } else { 0 }).sum()
    }
}

/// What a player puts up for trade, from its inventory (item, count, equipped count):
/// every item but caps, less what is equipped, at most `u16::MAX` each.
pub fn tradeable(items: impl IntoIterator<Item = (u32, i32, i32)>, caps: u32) -> Vec<(u32, u16)> {
    let mut out: Vec<(u32, u16)> = items.into_iter().filter(|(f, _, _)| *f != caps).filter_map(|(f, n, worn)| u16::try_from((n - worn).clamp(0, u16::MAX as i32)).ok().filter(|n| *n > 0).map(|n| (f, n))).collect();
    out.sort_unstable();
    out.dedup_by(|b, a| {
        if a.0 == b.0 {
            a.1 = a.1.saturating_add(b.1);
            true
        } else {
            false
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const STIM: u32 = 0x0001_5169;
    const CAPS: u32 = 0xF;

    #[test]
    fn what_is_up_for_trade() {
        let t = tradeable([(STIM, 3, 0), (CAPS, 500, 0), (0x100, 1, 1), (0x200, 2, 1), (0x300, 0, 0)], CAPS);
        assert_eq!(t, vec![(0x200, 1), (STIM, 3)]);
        assert_eq!(tradeable([(5, 2, 0), (5, 3, 0)], CAPS), vec![(5, 5)], "split stacks counted once");
    }

    #[test]
    fn stock_goes_in_pages() {
        let mine: Vec<(u32, u16)> = (0..(STOCK_PAGE as u32 * 2 + 5)).map(|i| (i + 1, 1)).collect();
        let mut owner = Trade::new(1, 4);
        let msgs = owner.stock_msgs(&mine, false);
        assert_eq!(msgs.len(), 3);
        assert!(owner.stock_msgs(&mine, false).is_empty(), "unchanged: not sent again");
        assert_eq!(owner.stock_msgs(&mine, true).len(), 3, "asked again");
        let mut buyer = Trade::new(0, 4);
        // Out of order, and the stock is only taken once whole.
        for m in msgs.iter().rev() {
            assert!(buyer.stock.is_empty());
            if let Msg::TradeStock { page, pages, items, .. } = m.clone() {
                buyer.stock_heard(page, pages, items);
            }
        }
        assert_eq!(buyer.stock, mine);
        // Nothing to trade still says so.
        let empty = owner.stock_msgs(&[], false);
        assert_eq!(empty, vec![Msg::TradeStock { nonce: 4, page: 0, pages: 1, items: vec![] }]);
    }

    /// The buyer's and the owner's books, with the owner holding `have` stimpaks.
    fn pair() -> (Trade, Trade) {
        let mut buyer = Trade::new(1, 7);
        let mut owner = Trade::new(0, 7);
        for m in owner.stock_msgs(&[(STIM, 5)], false) {
            if let Msg::TradeStock { page, pages, items, .. } = m {
                buyer.stock_heard(page, pages, items);
            }
        }
        (buyer, owner)
    }

    #[test]
    fn asked_priced_and_bought_per_item() {
        let (mut buyer, mut owner) = pair();
        assert!(buyer.request(STIM, 6).is_err(), "more than they have");
        assert!(buyer.request(STIM, 0).is_err());
        assert_eq!(buyer.request(STIM, 3), Ok(Msg::TradeRequest { nonce: 7, item: STIM, count: 3 }));
        assert!(buyer.buy(0, 1_000).is_err(), "no price yet");
        owner.request_heard(STIM, 3, 5, 75);
        assert_eq!(owner.offers, vec![Offer { item: STIM, count: 3, each: 75, sent: None }]);
        // The owner sets the price for one.
        owner.nudge(0, -25);
        owner.nudge(0, -1_000);
        assert_eq!(owner.offers[0].each, 1, "never under 1");
        owner.nudge(0, 39);
        let Some(Msg::TradePrice { each, .. }) = owner.price(0) else { panic!() };
        assert_eq!(each, 40);
        buyer.price_heard(STIM, 40);
        assert_eq!(buyer.total(0), Some(120), "40 each for 3");
        assert!(buyer.buy(0, 119).is_err(), "not enough caps");
        let (take, msg) = buyer.buy(0, 500).unwrap();
        assert_eq!(take, 120);
        assert_eq!(buyer.held(), 120);
        let Msg::TradeBuy { item, count, each, .. } = msg else { panic!() };
        let (sale, back) = owner.buy_heard(item, count, each, 5);
        assert_eq!(sale, Sale::Sell { item: STIM, count: 3, each: 40 });
        assert!(owner.offers.is_empty());
        let Msg::TradeSold { item, count, each, .. } = back else { panic!() };
        assert!(buyer.sold(item, count, each));
        assert!(!buyer.sold(item, count, each), "the items come once");
        assert_eq!(buyer.asks[0].state, AskState::Bought);
        assert_eq!(buyer.held(), 0);
    }

    #[test]
    fn a_buy_the_owner_cannot_honour_gives_the_caps_back() {
        let (mut buyer, mut owner) = pair();
        let _ = buyer.request(STIM, 2);
        owner.request_heard(STIM, 2, 5, 10);
        owner.price(0);
        buyer.price_heard(STIM, 10);
        let (_, msg) = buyer.buy(0, 100).unwrap();
        let Msg::TradeBuy { item, count, each, .. } = msg else { panic!() };
        // The owner sold or used them meanwhile.
        let (sale, back) = owner.buy_heard(item, count, each, 1);
        assert_eq!(sale, Sale::Refuse);
        let Msg::TradeRefused { item, count, each, .. } = back else { panic!() };
        assert_eq!(buyer.refused(item, count, each), Some(20));
        assert_eq!(buyer.refused(item, count, each), None, "given back once");
        assert_eq!(buyer.held(), 0);
    }

    #[test]
    fn a_changed_price_or_count_is_not_bought_at_the_old_one() {
        let (mut buyer, mut owner) = pair();
        let _ = buyer.request(STIM, 2);
        owner.request_heard(STIM, 2, 5, 10);
        owner.price(0);
        // The buyer pays 10 each but the owner has since asked 12.
        owner.nudge(0, 2);
        owner.price(0);
        let (sale, _) = owner.buy_heard(STIM, 2, 10, 5);
        assert_eq!(sale, Sale::Refuse);
        // Declined: the buyer cannot buy it.
        owner.decline(0);
        buyer.price_heard(STIM, 0);
        assert_eq!(buyer.asks[0].state, AskState::Declined);
        assert!(buyer.buy(0, 1_000).is_err());
        // Asked again for more: a fresh wait for a price.
        let _ = buyer.request(STIM, 4);
        assert_eq!(buyer.asks, vec![Ask { item: STIM, count: 4, state: AskState::Waiting }]);
        owner.request_heard(STIM, 9, 5, 10);
        assert_eq!(owner.offers[0].count, 5, "no more than the owner has");
        owner.request_heard(STIM, 1, 0, 10);
        assert!(owner.offers.is_empty(), "none left: no offer");
    }
}
