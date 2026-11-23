//! The co-op trade on the barter menu's screen (the owner's request, 2026-11-05: "trade
//! should look like the normal trade menu"). The menu's model ([`MenuModel`], marked
//! `coop`) is filled from the trade's book ([`super::trade::Trade`]) and drawn, keyed and
//! clicked exactly as an NPC's barter menu (`crate::menu`); its commands come here instead
//! of [`MenuModel::apply`], as they move nothing until the other player answers.
//!
//! - Left, this player's items; right, the other player's, as last heard. A row asked
//!   for carries the barter menu's put-up mark; its price column is the owner's price for
//!   one (a row not priced yet shows "--"). Each column's caps figure is that player's
//!   caps; the arrow and figure between them are the caps the priced asks would cost.
//! - On the other's row: Enter or a click asks for it ("How many?" for a stack), on a
//!   priced row buys it; Shift asks for the whole stack. On this player's row the other
//!   asked for: Enter or a click sets the price for one (the same prompt, "Price for one"),
//!   sent on Ok; Shift+Enter refuses it.
//! - A (Accept) sends every price not sent yet and buys every priced ask this player can
//!   pay for; X, Esc or Tab leave the trade.
//! - The line under the lists says where the selected item's trade stands, or what just
//!   happened.
//!
//! The caps of a buy are taken when bought and held until the owner confirms; given back
//! if refused or never answered (`super::trade`).

use super::trade::{AskState, Trade, PRICE_MAX};
use super::wire::Msg;
use crate::menu::barter::Prices;
use crate::menu::model::{self, Command, Listing, MenuModel, Mode, Quantity, Row, Side};
use fnv_sim::forms::FormStore;
use fnv_sim::FormId;
use std::collections::BTreeMap;

/// The prompt's line when it sets a price.
pub const PRICE_LABEL: &str = "Price for one (caps)";
/// How long a line of what happened stays under the lists (real seconds).
const EVENT_SECONDS: f32 = 5.0;

/// What a command asks of the game.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    Send(Msg),
    /// Take this many caps from this player (held for a buy).
    TakeCaps(u32),
    /// A line for the screen.
    Say(String),
    Close,
}

/// The barter screen opened for a trade with `name`, whose puppet is `puppet`.
pub fn open(name: &str, puppet: u32) -> MenuModel {
    let mut m = MenuModel::empty(Mode::Barter(FormId(puppet)), name.to_string());
    m.coop = true;
    m.focus = Side::Container;
    m
}

/// A row for `count` of `item`, as the barter menu lists a stack.
pub fn row_of(forms: &FormStore, item: u32, count: u16) -> Row {
    let id = FormId(item);
    let it = forms.item(id);
    Row {
        item: id,
        at: None,
        name: model::display_name(forms, id),
        count: count as i32,
        weight: it.and_then(|i| i.weight),
        value: it.and_then(|i| i.worth()),
        kind: it.map(|i| i.kind),
        equipped: false,
        quest: false,
        condition: None,
        icon: it.and_then(|i| i.icon.clone()),
        price: None,
        bartered: false,
        loaded: false,
        mods: 0,
        mods_value: 0,
        folder: None,
        reference: None,
        live: false,
    }
}

/// What the screen shows, from the book: the two lists, the trade's figures and the line
/// under them. `mine` is this player's stock (caps last, as [`super::my_stock`] gives it),
/// `event` a line of what happened with when it was said. Changes `m` only where it
/// differs, so the menu is drawn again only then.
pub fn fill(m: &mut MenuModel, book: &Trade, name: &str, mine: &[(u32, u16)], forms: &FormStore, event: Option<&(String, f32)>, now: f32) -> bool {
    let caps = model::CAPS.0;
    let own: Vec<(u32, u16)> = mine.iter().copied().filter(|(f, _)| *f != caps).collect();
    let my_caps: u32 = mine.iter().filter(|(f, _)| *f == caps).map(|(_, n)| *n as u32).sum();
    let theirs: Vec<(u32, u16)> = book.stock.iter().copied().filter(|(f, _)| *f != caps).collect();
    let their_caps = book.in_stock(caps) as u32;
    let mut lists = [Listing::default(), Listing::default()];
    let mut offered: [BTreeMap<model::Units, i32>; 2] = [BTreeMap::new(), BTreeMap::new()];
    let mut cost: u64 = 0;
    for (side, stock) in [(Side::Container, &theirs), (Side::Player, &own)] {
        let i = side as usize;
        let mut rows: Vec<Row> = Vec::new();
        for &(item, n) in stock.iter() {
            let mut r = row_of(forms, item, n);
            match side {
                Side::Container => {
                    if let Some(a) = book.asks.iter().find(|a| a.item == item) {
                        r.bartered = true;
                        r.price = match a.state {
                            AskState::Priced(e) | AskState::Buying(e) => Some(e as f32),
                            _ => None,
                        };
                        if let AskState::Priced(e) = a.state {
                            cost += e as u64 * a.count as u64;
                            offered[i].insert(r.units(), a.count as i32);
                        }
                    }
                }
                Side::Player => {
                    if let Some(o) = book.offers.iter().find(|o| o.item == item) {
                        r.bartered = true;
                        r.price = Some(o.each as f32);
                        if o.sent != Some(o.each) {
                            offered[i].insert(r.units(), o.count as i32);
                        }
                    }
                }
            }
            rows.push(r);
        }
        rows.retain(|r| m.category[i].holds_item(r.kind, forms.item(r.item)));
        rows.sort_by(|a, b| (!a.bartered, &a.name, a.item).cmp(&(!b.bartered, &b.name, b.item)));
        let weight = rows.iter().map(|r| r.weight.unwrap_or(0.0) as f64 * r.count as f64).sum();
        let value = rows.iter().map(|r| r.value.unwrap_or(0) as i64 * r.count as i64).sum();
        lists[i] = Listing { rows, weight, value, hidden: 0 };
    }
    let trade = model::Trade {
        merchant: match m.mode {
            Mode::Barter(r) => r,
            _ => FormId(0),
        },
        stock: FormId(0),
        hidden: Default::default(),
        caps: model::CAPS,
        discount: 0,
        skill: 0.0,
        prices: Prices::default(),
        offered,
        purse: [their_caps as i32, my_caps as i32],
        balance: -(cost.min(i64::MAX as u64) as i64),
        sells: None,
        withheld: Vec::new(),
        mult_set: [None; 2],
    };
    let mut changed = false;
    // The selection stays on the same item when the rows move (as the barter menu's).
    let keep = [Side::Container, Side::Player].map(|side| m.selected_row(side).map(|r| r.item));
    if m.lists != lists {
        m.lists = lists;
        changed = true;
    }
    for side in [Side::Container, Side::Player] {
        let i = side as usize;
        let n = m.lists[i].rows.len();
        let at = keep[i].and_then(|item| m.lists[i].rows.iter().position(|r| r.item == item));
        let sel = at.unwrap_or(m.selected[i]).min(n.saturating_sub(1));
        let first = model::keep_in_view(m.scroll[i], sel, n, m.page());
        if (sel, first) != (m.selected[i], m.scroll[i]) {
            (m.selected[i], m.scroll[i]) = (sel, first);
            changed = true;
        }
    }
    if m.trade.as_ref() != Some(&trade) {
        m.trade = Some(trade);
        changed = true;
    }
    let status = Some(match event.filter(|(_, at)| now - at < EVENT_SECONDS) {
        Some((line, _)) => line.clone(),
        None => hint(m, book, name),
    });
    if m.status != status {
        m.status = status;
        changed = true;
    }
    changed
}

/// Where the selected item's trade stands.
fn hint(m: &MenuModel, book: &Trade, name: &str) -> String {
    let Some(r) = m.selected_row(m.focus) else {
        return match m.focus {
            Side::Container => format!("{name} has nothing to trade"),
            Side::Player => "You have nothing to trade".into(),
        };
    };
    let item = r.item.0;
    match m.focus {
        Side::Container => match book.asks.iter().find(|a| a.item == item) {
            None => format!("Enter: ask {name} for {}", r.name),
            Some(a) => match a.state {
                AskState::Waiting => format!("Asked for {}: waiting for {name}'s price", a.count),
                AskState::Priced(e) => format!("{} for {e} caps each ({} caps): Enter or A buys", a.count, e as u64 * a.count as u64),
                AskState::Buying(_) => format!("Bought: waiting for {name} to hand it over"),
                AskState::Declined => format!("{name} will not sell this: Enter asks again"),
                AskState::Bought => "Bought".into(),
            },
        },
        Side::Player => match book.offers.iter().find(|o| o.item == item) {
            None => format!("{name} has not asked for this"),
            Some(o) if o.sent == Some(o.each) => format!("{name} asks for {}: {} caps each sent. Enter: change, Shift+Enter: refuse", o.count, o.each),
            Some(o) => format!("{name} asks for {}. Enter: set a price (now {} each), Shift+Enter: refuse", o.count, o.each),
        },
    }
}

/// The most the price prompt goes to for an item worth `value`, its price now `each`.
pub fn price_max(each: u32, value: u32) -> i32 {
    (each.max(value).max(5) as u64 * 4).clamp(20, PRICE_MAX as u64) as i32
}

fn select(m: &mut MenuModel, side: Side, row: i64) {
    let i = side as usize;
    let n = m.lists[i].rows.len();
    m.focus = side;
    m.selected[i] = row.clamp(0, n.saturating_sub(1) as i64) as usize;
    m.scroll[i] = model::keep_in_view(m.scroll[i], m.selected[i], n, m.page());
}

/// A command of the barter screen (keys, clicks, the wheel) on the trade. `my_caps` is
/// what this player carries.
pub fn command(m: &mut MenuModel, book: &mut Trade, cmd: Command, my_caps: u32) -> Vec<Effect> {
    let mut fx = Vec::new();
    // The prompt open: only its own commands (Close cancels it).
    if let Some(mut q) = m.quantity {
        match cmd {
            Command::AmountStep(by) => q.amount = (q.amount + by).clamp(1, q.max),
            Command::Amount(n) => q.amount = n.clamp(1, q.max),
            Command::AmountOk => {
                m.quantity = None;
                let item = m.lists[q.side as usize].rows.get(q.row).map(|r| r.item.0);
                match (item, q.label.is_some()) {
                    (Some(item), true) => {
                        if let Some(i) = book.offers.iter().position(|o| o.item == item) {
                            book.set_price(i, q.amount as u32);
                            fx.extend(book.price(i).map(Effect::Send));
                        }
                    }
                    (Some(item), false) => ask(book, item, q.amount.clamp(1, u16::MAX as i32) as u16, &mut fx),
                    (None, _) => {}
                }
                return fx;
            }
            Command::AmountCancel | Command::Close => {
                m.quantity = None;
                return fx;
            }
            _ => return fx,
        }
        m.quantity = Some(q);
        return fx;
    }
    let f = m.focus;
    let sel = m.selected[f as usize] as i64;
    match cmd {
        Command::Up => select(m, f, sel - 1),
        Command::Down => select(m, f, sel + 1),
        Command::PageUp => select(m, f, sel - m.page() as i64),
        Command::PageDown => select(m, f, sel + m.page() as i64),
        Command::Scroll(n) => select(m, f, sel + n as i64),
        Command::Focus(side) => select(m, side, m.selected[side as usize] as i64),
        Command::Hover(side, row) => {
            select(m, side, row as i64);
            m.pointer_pick = true;
        }
        Command::Select(side, row) if side == f && row as i64 == sel => return command(m, book, Command::One, my_caps),
        Command::Select(side, row) => select(m, side, row as i64),
        Command::Category(side, by) => m.category[side as usize] = m.category[side as usize].step(by),
        Command::SetCategory(side, c) => m.category[side as usize] = c,
        Command::One | Command::Stack => {
            let whole = cmd == Command::Stack;
            let Some(r) = m.selected_row(f).cloned() else { return fx };
            let item = r.item.0;
            match f {
                Side::Container => match book.asks.iter().position(|a| a.item == item) {
                    Some(i) if !whole && matches!(book.asks[i].state, AskState::Priced(_)) => buy(book, i, my_caps, &mut fx),
                    Some(i) if matches!(book.asks[i].state, AskState::Buying(_)) => fx.push(Effect::Say("Already bought: waiting for the items".into())),
                    _ => {
                        let have = book.in_stock(item);
                        if whole || have <= 1 {
                            ask(book, item, have.max(1), &mut fx);
                        } else {
                            m.quantity = Some(Quantity { cmd: Command::One, side: f, row: m.selected[f as usize], max: have as i32, amount: have as i32, label: None });
                        }
                    }
                },
                Side::Player => match book.offers.iter().position(|o| o.item == item) {
                    Some(i) if whole => {
                        fx.extend(book.decline(i).map(Effect::Send));
                        fx.push(Effect::Say(format!("You will not sell {}", r.name)));
                    }
                    Some(i) => {
                        let o = book.offers[i];
                        let value = r.value.unwrap_or(1).max(1) as u32;
                        m.quantity = Some(Quantity { cmd: Command::One, side: f, row: m.selected[f as usize], max: price_max(o.each, value), amount: o.each as i32, label: Some(PRICE_LABEL) });
                    }
                    None => fx.push(Effect::Say("Nobody has asked for that".into())),
                },
            }
        }
        Command::Accept => {
            let mut done = false;
            for i in 0..book.offers.len() {
                if book.offers[i].sent != Some(book.offers[i].each) {
                    fx.extend(book.price(i).map(Effect::Send));
                    done = true;
                }
            }
            let mut caps = my_caps;
            for i in 0..book.asks.len() {
                if let Some(total) = book.total(i) {
                    let before = fx.len();
                    buy(book, i, caps, &mut fx);
                    if fx[before..].iter().any(|e| matches!(e, Effect::TakeCaps(_))) {
                        caps -= total;
                    }
                    done = true;
                }
            }
            if !done {
                fx.push(Effect::Say("Nothing to settle: ask for an item or set a price".into()));
            }
        }
        Command::Close => fx.push(Effect::Close),
        _ => {}
    }
    fx
}

fn ask(book: &mut Trade, item: u32, count: u16, fx: &mut Vec<Effect>) {
    match book.request(item, count) {
        Ok(msg) => fx.push(Effect::Send(msg)),
        Err(why) => fx.push(Effect::Say(format!("Cannot ask: {why}"))),
    }
}

fn buy(book: &mut Trade, i: usize, caps: u32, fx: &mut Vec<Effect>) {
    match book.buy(i, caps) {
        Ok((take, msg)) => {
            fx.push(Effect::TakeCaps(take));
            fx.push(Effect::Send(msg));
        }
        Err(why) => fx.push(Effect::Say(format!("Cannot buy: {why}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::model::Category;

    const STIM: u32 = 0x0001_5169;
    const ROUND: u32 = 0x0000_4241;

    fn forms() -> FormStore {
        FormStore::default()
    }

    /// The buyer's and the owner's books and screens: the owner has 5 stimpaks and 20
    /// rounds and 300 caps, the buyer 100 caps.
    fn pair() -> ((Trade, MenuModel), (Trade, MenuModel)) {
        let mut buyer = Trade::new(1, 7);
        let owner = Trade::new(0, 7);
        buyer.stock = vec![(model::CAPS.0, 300), (ROUND, 20), (STIM, 5)];
        let (mut bm, mut om) = (open("Owner", 0xFF00_0001), open("Buyer", 0xFF00_0002));
        assert_eq!(bm.category[0], Category::All);
        let f = forms();
        fill(&mut bm, &buyer, "Owner", &[(model::CAPS.0, 100)], &f, None, 0.0);
        fill(&mut om, &owner, "Buyer", &[(ROUND, 20), (STIM, 5), (model::CAPS.0, 300)], &f, None, 0.0);
        ((buyer, bm), (owner, om))
    }

    fn sent(fx: &[Effect]) -> Vec<Msg> {
        fx.iter().filter_map(|e| if let Effect::Send(m) = e { Some(m.clone()) } else { None }).collect()
    }

    #[test]
    fn the_screen_shows_both_inventories_and_caps() {
        let ((_, bm), (_, om)) = pair();
        let theirs: Vec<u32> = bm.lists[Side::Container as usize].rows.iter().map(|r| r.item.0).collect();
        assert_eq!(theirs.len(), 2, "caps are a figure, not a row: {theirs:?}");
        assert!(bm.lists[Side::Player as usize].rows.is_empty());
        assert_eq!(bm.trade.as_ref().unwrap().purse, [300, 100]);
        assert_eq!(om.trade.as_ref().unwrap().purse, [0, 300]);
        assert!(bm.coop && matches!(bm.mode, Mode::Barter(_)));
        assert!(bm.status.as_deref().unwrap().starts_with("Enter: ask Owner for"));
    }

    #[test]
    fn ask_price_and_buy_through_the_barter_commands() {
        let ((mut buyer, mut bm), (mut owner, mut om)) = pair();
        let f = forms();
        // The buyer picks the stimpaks (a stack: "How many?"), asks for 3.
        let row = bm.lists[0].rows.iter().position(|r| r.item.0 == STIM).unwrap();
        command(&mut bm, &mut buyer, Command::Hover(Side::Container, row), 100);
        assert!(command(&mut bm, &mut buyer, Command::One, 100).is_empty());
        let q = bm.quantity.unwrap();
        assert_eq!((q.max, q.amount, q.label), (5, 5, None));
        command(&mut bm, &mut buyer, Command::AmountStep(-2), 100);
        let fx = command(&mut bm, &mut buyer, Command::AmountOk, 100);
        assert_eq!(sent(&fx), vec![Msg::TradeRequest { nonce: 7, item: STIM, count: 3 }]);
        fill(&mut bm, &buyer, "Owner", &[(model::CAPS.0, 100)], &f, None, 0.0);
        let r = bm.selected_row(Side::Container).unwrap();
        assert!(r.bartered && r.price.is_none(), "asked, no price yet");
        // The owner hears it: its row is marked; Enter opens the price prompt.
        owner.request_heard(STIM, 3, 5, 75);
        fill(&mut om, &owner, "Buyer", &[(ROUND, 20), (STIM, 5)], &f, None, 0.0);
        let mine = om.lists[1].rows.iter().position(|r| r.item.0 == STIM).unwrap();
        assert_eq!(mine, 0, "asked rows first");
        command(&mut om, &mut owner, Command::Hover(Side::Player, mine), 300);
        command(&mut om, &mut owner, Command::One, 300);
        let q = om.quantity.unwrap();
        assert_eq!((q.amount, q.label), (75, Some(PRICE_LABEL)));
        command(&mut om, &mut owner, Command::Amount(30), 300);
        let fx = command(&mut om, &mut owner, Command::AmountOk, 300);
        assert_eq!(sent(&fx), vec![Msg::TradePrice { nonce: 7, item: STIM, each: 30 }]);
        // The buyer sees the price, the arrow's figure is the cost; A buys.
        buyer.price_heard(STIM, 30);
        fill(&mut bm, &buyer, "Owner", &[(model::CAPS.0, 100)], &f, None, 0.0);
        assert_eq!(bm.selected_row(Side::Container).unwrap().price, Some(30.0));
        assert_eq!(bm.trade.as_ref().unwrap().balance, -90);
        assert!(bm.trade.as_ref().unwrap().affordable());
        let fx = command(&mut bm, &mut buyer, Command::Accept, 100);
        assert_eq!(fx[0], Effect::TakeCaps(90));
        assert_eq!(sent(&fx), vec![Msg::TradeBuy { nonce: 7, item: STIM, count: 3, each: 30 }]);
        assert_eq!(buyer.held(), 90);
    }

    #[test]
    fn refusing_closing_and_the_prompt_cancel() {
        let ((mut buyer, mut bm), (mut owner, mut om)) = pair();
        let f = forms();
        // Shift asks for the whole stack at once.
        let row = bm.lists[0].rows.iter().position(|r| r.item.0 == ROUND).unwrap();
        command(&mut bm, &mut buyer, Command::Hover(Side::Container, row), 100);
        let fx = command(&mut bm, &mut buyer, Command::Stack, 100);
        assert_eq!(sent(&fx), vec![Msg::TradeRequest { nonce: 7, item: ROUND, count: 20 }]);
        owner.request_heard(ROUND, 20, 20, 1);
        fill(&mut om, &owner, "Buyer", &[(ROUND, 20)], &f, None, 0.0);
        command(&mut om, &mut owner, Command::Focus(Side::Player), 0);
        // A prompt cancelled sends nothing; Shift+Enter refuses.
        command(&mut om, &mut owner, Command::One, 0);
        assert!(command(&mut om, &mut owner, Command::Close, 0).is_empty());
        assert!(om.quantity.is_none());
        let fx = command(&mut om, &mut owner, Command::Stack, 0);
        assert_eq!(sent(&fx), vec![Msg::TradePrice { nonce: 7, item: ROUND, each: 0 }]);
        assert!(owner.offers.is_empty());
        // Accept with nothing to settle says so; X leaves.
        assert!(matches!(command(&mut om, &mut owner, Command::Accept, 0)[..], [Effect::Say(_)]));
        assert_eq!(command(&mut om, &mut owner, Command::Close, 0), vec![Effect::Close]);
    }

    #[test]
    fn the_price_prompt_reaches_past_the_value() {
        assert_eq!(price_max(1, 1), 20);
        assert_eq!(price_max(75, 75), 300);
        assert_eq!(price_max(PRICE_MAX, 1), PRICE_MAX as i32);
    }
}
