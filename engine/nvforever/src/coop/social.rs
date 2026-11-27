//! The players' actions (`nvf_play::coop_tables`) on the session's reliable stream
//! (`nvf_coop::action`), and the host's checks on the guests' ones.
//!
//! - [`pump`], each frame: this game's actions out ([`Tables::actions_out`]), each as one
//!   reliable item; the others' in ([`Tables::actions_in`]). The host takes a guest's
//!   action (its guard admitted it: [`host_rules`]), stamps the sender on it
//!   (`nvf_coop::action::relay`) and passes it on to every other guest; a guest takes
//!   the host's items as they come, the sender already stamped.
//! - [`host_rules`]: what the host lets through, per game, beyond the guard's parse and
//!   blob size: a rate per guest, and the game's own check of what the blob says
//!   (module tests). A duel's or a trade's messages must name one the host saw begin
//!   ([`Open`]: the invitations it let through and its own).
//! - [`EmoteBounds`]: a guest's state names an emote of this build's catalog.
//!
//! Log lines: `co-op actions: …` (a refused send, the first of each game heard).

use super::{label, Coop};
use bevy::prelude::*;
use nvf_coop::action::{self, Action, Actions, GameRule, ANYONE};
use nvf_coop::guard::{Ctx, Rate, Reason, Rule, Subject, Verdict};
use nvf_coop::net::MAX_GUESTS;
use nvf_play::coop_social::duel::DELAY_MS;
use nvf_play::coop_social::trade::{cost, PRICE_MAX};
use nvf_play::coop_social::wire::Msg;
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use nvf_play::coop_tables::{game, Tables, R};
use nvf_play::world_marks::PingMsg;

/// Players' ids: the host 0, the guests 1 to [`MAX_GUESTS`].
fn a_player(id: u32) -> bool {
    id <= MAX_GUESTS as u32
}

/// The duels and trades under way, as the host saw them begin: (nonce, inviter, invited),
/// the newest last, at most [`OPEN_PER_INVITER`] of each inviter's (a new one forgets only
/// the inviter's own oldest, never another player's); and when each player last invited. Shared by the
/// guard's rule ([`Social`]) and [`pump`] (the host's own invitations) [design: a guest
/// cannot speak for a duel or a trade it is not in].
#[derive(Clone, Default)]
pub(super) struct Open(Arc<Mutex<OpenInner>>);

#[derive(Default)]
struct OpenInner {
    sessions: VecDeque<(u16, u8, u8)>,
    invited_at: BTreeMap<u8, Instant>,
}

/// How many duels and trades the host remembers for each inviter [design: a duel and a
/// trade at once; the players are few ([`MAX_GUESTS`]), so the whole is bounded too].
const OPEN_PER_INVITER: usize = 2;
/// The least time between one player's invitations [design: one a second].
const INVITE_GAP: Duration = Duration::from_secs(1);

impl Open {
    fn lock(&self) -> MutexGuard<'_, OpenInner> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// `by` invited `to` to the session `nonce`.
    pub(super) fn invited(&self, nonce: u16, by: u8, to: u8) {
        let mut o = self.lock();
        o.sessions.retain(|e| !(e.0 == nonce && e.1 == by));
        o.sessions.push_back((nonce, by, to));
        // Only this inviter's own oldest go: one player's invitations never end another
        // pair's duel or trade.
        while o.sessions.iter().filter(|e| e.1 == by).count() > OPEN_PER_INVITER {
            if let Some(i) = o.sessions.iter().position(|e| e.1 == by) {
                o.sessions.remove(i);
            }
        }
    }

    /// `a` and `b` are the two players of the session `nonce`.
    fn between(&self, nonce: u16, a: u8, b: u8) -> bool {
        self.lock().sessions.iter().any(|&(n, x, y)| n == nonce && ((x, y) == (a, b) || (x, y) == (b, a)))
    }

    /// `a` is one of the session's players.
    fn with(&self, nonce: u16, a: u8) -> bool {
        self.lock().sessions.iter().any(|&(n, x, y)| n == nonce && (x == a || y == a))
    }

    /// `by` may invite now (and is counted as having invited): at most one a [`INVITE_GAP`].
    fn may_invite(&self, by: u8, now: Instant) -> bool {
        let mut o = self.lock();
        if o.invited_at.get(&by).is_some_and(|&t| now.saturating_duration_since(t) < INVITE_GAP) {
            return false;
        }
        o.invited_at.insert(by, now);
        true
    }
}

/// The interaction wheel's messages [design]:
/// - one a guest can read, for another player;
/// - an invitation under the sender's own nonce (its id in the top 4 bits, as
///   `nvf_play::coop_social` makes them), at most one a second;
/// - any other message of a duel or a trade between the sender and that player that the
///   host saw begin; a thrown body's, of one the sender is in, since it goes to everyone;
/// - a duel's countdown within the duel's own (`nvf_play::coop_social::duel::DELAY_MS`), its
///   winner one of its two players or nobody, a thrown body's velocity within what a duel
///   throws (`nvf_play::coop_social::duel::THROW`, with room).
struct Social(Open);

/// The fastest a duel's thrown body may go, game units a second, along each axis [design].
const THROW_MAX: i32 = 1000;

impl GameRule for Social {
    fn check(&self, ctx: &Ctx, from: u8, table: u32, blob: &[u8]) -> Verdict {
        let Some(m) = Msg::decode(blob) else { return Verdict::Drop(Reason::Rule("social: unreadable")) };
        if !a_player(table) || table == u32::from(from) {
            return Verdict::Drop(Reason::Rule("social: for no other player"));
        }
        let to = table as u8;
        let nonce = m.nonce();
        let known = match m {
            Msg::Invite { .. } => {
                if nonce >> 12 != u16::from(from & 0xF) {
                    return Verdict::Drop(Reason::Rule("social: an invitation under another's nonce"));
                }
                if !self.0.may_invite(from, ctx.now) {
                    return Verdict::Drop(Reason::Rate);
                }
                self.0.invited(nonce, from, to);
                return Verdict::Accept;
            }
            Msg::Thrown { .. } => self.0.with(nonce, from),
            _ => self.0.between(nonce, from, to),
        };
        if !known {
            return Verdict::Drop(Reason::Rule("social: no such duel or trade"));
        }
        let item_ok = |item: u32| ctx.forms.is_none_or(|f| f.item(item));
        // A price is 1 to PRICE_MAX (a price of 0 is the owner declining); what a count of
        // them costs fits in the caps a player holds (`trade::cost`).
        let priced = |item: u32, count: u16, each: u32| count >= 1 && (1..=PRICE_MAX).contains(&each) && cost(each, count).is_some() && item_ok(item);
        let in_bounds = match m {
            Msg::TradeStock { items, .. } => items.iter().all(|&(item, count)| count >= 1 && item_ok(item)),
            Msg::TradeRequest { item, count, .. } => count >= 1 && item_ok(item),
            Msg::TradePrice { item, each, .. } => each <= PRICE_MAX && item_ok(item),
            Msg::TradeBuy { item, count, each, .. } | Msg::TradeSold { item, count, each, .. } | Msg::TradeRefused { item, count, each, .. } => priced(item, count, each),
            Msg::DuelFreeze { delay_ms, .. } => (DELAY_MS.0..=DELAY_MS.1).contains(&delay_ms),
            Msg::DuelOver { winner, .. } => winner == from || winner == to || winner == u8::MAX, // 255: nobody
            Msg::Thrown { velocity, .. } => velocity.iter().all(|v| v.abs() <= THROW_MAX),
            // The rest carry nothing to bound: a session's own words.
            Msg::Invite { .. } | Msg::Answer { .. } | Msg::Quit { .. } | Msg::DuelReady { .. } | Msg::DuelHit { .. } | Msg::TradeBrowse { .. } => true,
        };
        if !in_bounds {
            return Verdict::Drop(Reason::Bounds);
        }
        Verdict::Accept
    }
}

/// A guest's state: its emote one of this build's (`nvf_play::emotes::CATALOG`; 0 none),
/// in a phase there is (`nvf_coop::net::emote`) [design: the others' games look the emote
/// up by its number].
pub(super) struct EmoteBounds;

impl Rule for EmoteBounds {
    fn check(&self, _: &Ctx, subject: &Subject) -> Verdict {
        if let Subject::Msg(nvf_coop::net::Msg::State { state, .. }) = subject {
            let e = &state.emote;
            if usize::from(e.id) > nvf_play::emotes::CATALOG.len() || e.phase > nvf_coop::net::emote::ONCE {
                return Verdict::Drop(Reason::Bounds);
            }
        }
        Verdict::Accept
    }
}

/// A ping: for everyone, readable, at a place the host knows, its point near where the
/// guest last stood [design: a ping is something the guest can see; the guard's
/// `Reason::Moved` reach is the guest's own movement, this is the ping's].
struct Ping;

/// How far from the guest's last state a ping's point may be (game units) [design: past a
/// long sight line].
const PING_REACH: f32 = 30_000.0;

impl GameRule for Ping {
    fn check(&self, ctx: &Ctx, _: u8, table: u32, blob: &[u8]) -> Verdict {
        let Some(p) = PingMsg::decode(blob) else { return Verdict::Drop(Reason::Rule("ping: unreadable")) };
        if table != game::EVERYONE {
            return Verdict::Drop(Reason::Rule("ping: not for everyone"));
        }
        if p.clear {
            return Verdict::Accept;
        }
        let place = if p.interior != 0 { nvf_coop::net::Place::Interior(p.interior) } else { nvf_coop::net::Place::Exterior(p.world) };
        if ctx.forms.is_some_and(|f| !f.place(place)) {
            return Verdict::Drop(Reason::Bounds);
        }
        if let Some(last) = ctx.last {
            let d = (0..3).map(|i| (p.pos[i] - last.feet[i]).powi(2)).sum::<f32>().sqrt();
            if d > PING_REACH {
                return Verdict::Drop(Reason::Moved);
            }
        }
        Verdict::Accept
    }
}

/// The actor a guest talks to: table 0, one reference (0: none), a placed actor the host
/// knows. Not measured against the guest: the host's rule sees the actor's place in the
/// plugins, not where it is now (a companion or a scheduled NPC is far from it) [design].
struct Talk;

impl GameRule for Talk {
    fn check(&self, ctx: &Ctx, _: u8, table: u32, blob: &[u8]) -> Verdict {
        let mut r = R(blob);
        let (0, Some(actor), true) = (table, r.u32(), r.done()) else { return Verdict::Drop(Reason::Rule("talk: not one reference")) };
        if actor == 0 {
            return Verdict::Accept; // the talk is over
        }
        let Some(forms) = ctx.forms else { return Verdict::Accept };
        if forms.actor(actor) {
            return Verdict::Accept;
        }
        // A reference made at run time (0xFF...) is in no plugin: the host's table cannot
        // judge it, and the guest did nothing wrong in naming it.
        if actor >> 24 == 0xFF {
            return Verdict::Drop(Reason::Unknown);
        }
        Verdict::Drop(Reason::Rule("talk: not a placed actor"))
    }
}

/// The host's checks on actions, for its guard ([`Actions::kind`]).
pub(super) fn host_rules(open: Open) -> Actions {
    let mut a = Actions::default();
    // [design: a wheel's exchange is a few messages a second; a trade's stock pages come
    // in a burst.]
    a.register(game::SOCIAL, Rate { burst: 24.0, per_sec: 12.0 }, Box::new(Social(open)));
    // [design: a ping a second is plenty; a few at once to place one and clear it.]
    a.register(game::PING, Rate { burst: 4.0, per_sec: 1.0 }, Box::new(Ping));
    // [design: told on a change and again every few seconds.]
    a.register(game::TALK, Rate { burst: 4.0, per_sec: 2.0 }, Box::new(Talk));
    a
}

/// The actions out and in (module notes), and the session [`Tables`] shows.
pub(super) fn pump(mut coop: ResMut<Coop>, mut tables: ResMut<Tables>, mut heard: Local<Vec<u8>>) {
    let coop = &mut *coop;
    let inbox = std::mem::take(&mut coop.actions_in);
    let Some(link) = coop.link.as_ref() else {
        if tables.on {
            *tables = Tables::default();
        }
        return;
    };
    let me = link.id();
    let host = link.is_host();
    let names = coop.peers.iter().map(|(&id, p)| (id, label(id, &p.name))).chain(me.map(|id| (id, coop.name.clone()))).collect();
    let on = me.is_some();
    if tables.on != on || tables.me != me || tables.host != host || tables.names != names {
        (tables.on, tables.me, tables.host, tables.names) = (on, me, host, names);
    }
    if !on {
        tables.actions_out.clear();
        return;
    }
    for (table, g, blob) in std::mem::take(&mut tables.actions_out) {
        // A guest's sender byte is never read: the host stamps it.
        let from = if host { me.unwrap_or(0) } else { ANYONE };
        // The host's own invitations open a duel or trade its checks know of.
        if host && g == game::SOCIAL {
            if let Some(Msg::Invite { nonce, .. }) = Msg::decode(&blob) {
                coop.social_open.invited(nonce, from, table as u8);
            }
        }
        let Some(item) = (Action { game: g, from, table, blob }).encode() else {
            warn!("co-op actions: an action of game {g} is too long to send");
            continue;
        };
        if !link.send_reliable(None, item) {
            warn!("co-op actions: the stream refused an action of game {g} (full)");
        }
    }
    for (id, item) in inbox {
        let Some(a) = Action::decode(&item) else { continue };
        let a = if host {
            // Admitted by the guard already; to the other guests, the sender stamped.
            let a = action::relay(id, a);
            if let Some(item) = a.encode() {
                for &guest in coop.peers.keys().filter(|&&g| g != id && g != 0) {
                    if !link.send_reliable(Some(guest), item.clone()) {
                        warn!("co-op actions: player {}'s stream refused a relayed action (full)", guest + 1);
                    }
                }
            }
            a
        } else {
            a
        };
        if !heard.contains(&a.game) {
            heard.push(a.game);
            info!("co-op actions: the first of game {} heard, from player {}", a.game, a.from + 1);
        }
        tables.actions_in.push((a.from, a.table, a.game, a.blob));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvf_coop::net::{EmoteState, Place, PlayerState};
    use nvf_play::coop_social::wire::Kind;

    fn check(acts: &Actions, from: u8, g: u8, table: u32, blob: Vec<u8>, now: Instant) -> Verdict {
        let item = Action { game: g, from: ANYONE, table, blob }.encode().unwrap();
        acts.check(&Ctx { from_id: Some(from), kind: Actions::kind(), now, last: None, forms: None }, &Subject::Item(&item))
    }

    const NO_SUCH: Verdict = Verdict::Drop(Reason::Rule("social: no such duel or trade"));

    #[test]
    fn the_host_takes_a_guests_social_messages_only_for_another_player() {
        let acts = host_rules(Open::default());
        let now = Instant::now();
        let invite = |nonce| Msg::Invite { kind: Kind::Duel, nonce }.encode();
        assert_eq!(check(&acts, 1, game::SOCIAL, 1, invite(1 << 12 | 1), now), Verdict::Drop(Reason::Rule("social: for no other player")), "to itself");
        assert_eq!(check(&acts, 1, game::SOCIAL, 9, invite(1 << 12 | 1), now), Verdict::Drop(Reason::Rule("social: for no other player")), "no such player");
        assert_eq!(check(&acts, 1, game::SOCIAL, 0, vec![99, 0, 0], now), Verdict::Drop(Reason::Rule("social: unreadable")));
        assert_eq!(check(&acts, 1, game::SOCIAL, 2, invite(2 << 12 | 1), now), Verdict::Drop(Reason::Rule("social: an invitation under another's nonce")));
        assert_eq!(check(&acts, 1, game::SOCIAL, 2, invite(1 << 12 | 1), now), Verdict::Accept);
        assert_eq!(check(&acts, 1, game::SOCIAL, 3, invite(1 << 12 | 2), now), Verdict::Drop(Reason::Rate), "one invitation a second");
        assert_eq!(check(&acts, 1, game::SOCIAL, 3, invite(1 << 12 | 2), now + Duration::from_secs(1)), Verdict::Accept);
    }

    #[test]
    fn a_duel_or_trade_is_spoken_for_only_by_its_two_players() {
        let open = Open::default();
        let acts = host_rules(open.clone());
        let now = Instant::now();
        let nonce = 2 << 12 | 7;
        let answer = Msg::Answer { nonce, yes: true }.encode();
        assert_eq!(check(&acts, 1, game::SOCIAL, 2, answer.clone(), now), NO_SUCH, "before any invitation");
        assert_eq!(check(&acts, 2, game::SOCIAL, 1, Msg::Invite { kind: Kind::Trade, nonce }.encode(), now), Verdict::Accept);
        assert_eq!(check(&acts, 1, game::SOCIAL, 2, answer.clone(), now), Verdict::Accept, "the invited answers");
        assert_eq!(check(&acts, 3, game::SOCIAL, 2, answer, now), NO_SUCH, "a third player cannot");
        assert_eq!(check(&acts, 2, game::SOCIAL, 1, Msg::TradeRequest { nonce, item: 0xF, count: 1 }.encode(), now), Verdict::Accept);
        assert_eq!(check(&acts, 3, game::SOCIAL, 1, Msg::Quit { nonce }.encode(), now), NO_SUCH);
        // The host's own invitation (`pump` records it).
        open.invited(5, 0, 3);
        assert_eq!(check(&acts, 3, game::SOCIAL, 0, Msg::Answer { nonce: 5, yes: false }.encode(), now), Verdict::Accept);
        // Bounded per inviter: one player's many invitations forget its own oldest, never
        // another pair's session.
        for n in 0..200u16 {
            open.invited(100 + n, 4, 5);
        }
        assert_eq!(check(&acts, 3, game::SOCIAL, 0, Msg::Quit { nonce: 5 }.encode(), now), Verdict::Accept, "the host's invitation to player 3 is still open");
        assert_eq!(check(&acts, 1, game::SOCIAL, 2, answer_to(nonce), now), Verdict::Accept, "so is the pair's trade");
        assert_eq!(check(&acts, 5, game::SOCIAL, 4, Msg::Quit { nonce: 100 }.encode(), now), NO_SUCH, "the spammer's oldest are gone");
        assert_eq!(check(&acts, 5, game::SOCIAL, 4, Msg::Quit { nonce: 299 }.encode(), now), Verdict::Accept, "its newest stay");
        assert_eq!(open.lock().sessions.iter().filter(|e| e.1 == 4).count(), OPEN_PER_INVITER);
    }

    fn answer_to(nonce: u16) -> Vec<u8> {
        Msg::Answer { nonce, yes: true }.encode()
    }

    /// The items 0x100 to 0x1FF are a player's; the placed actors 0x10C1DB and 0x10C1DC.
    struct Forms;

    impl nvf_coop::guard::FormCheck for Forms {
        fn carriable(&self, _: u32) -> bool {
            false
        }
        fn wearable(&self, _: u32) -> bool {
            false
        }
        fn furniture(&self, _: u32) -> bool {
            false
        }
        fn item(&self, form: u32) -> bool {
            (0x100..0x200).contains(&form) || form == 0xF
        }
        fn actor(&self, reference: u32) -> bool {
            matches!(reference, 0x0010_C1DB | 0x0010_C1DC)
        }
    }

    fn check_forms(acts: &Actions, from: u8, g: u8, table: u32, blob: Vec<u8>, now: Instant) -> Verdict {
        let item = Action { game: g, from: ANYONE, table, blob }.encode().unwrap();
        let last = nvf_coop::guard::LastState { place: Place::Exterior(0xDA726), feet: [0.0; 3], weapon: 0, weapon_before: None, at: now };
        acts.check(&Ctx { from_id: Some(from), kind: Actions::kind(), now, last: Some(last), forms: Some(&Forms) }, &Subject::Item(&item))
    }

    #[test]
    fn a_trades_items_prices_and_totals_are_bounded() {
        let open = Open::default();
        let acts = host_rules(open.clone());
        let now = Instant::now();
        let nonce = 1 << 12 | 3;
        open.invited(nonce, 1, 2);
        // Each a second on from the last: this is the bounds' test, not the rate's.
        let n = std::cell::Cell::new(0u64);
        let ok = |m: Msg| {
            n.set(n.get() + 1);
            check_forms(&acts, 1, game::SOCIAL, 2, m.encode(), now + Duration::from_secs(n.get()))
        };
        let refused = Verdict::Drop(Reason::Bounds);
        // Stock: items a player can hold, each at least one.
        assert_eq!(ok(Msg::TradeStock { nonce, page: 0, pages: 1, items: vec![(0x100, 3), (0xF, 40)] }), Verdict::Accept);
        assert_eq!(ok(Msg::TradeStock { nonce, page: 0, pages: 1, items: vec![(0x100, 0)] }), refused, "none of it");
        assert_eq!(ok(Msg::TradeStock { nonce, page: 0, pages: 1, items: vec![(0x100, 1), (0x9999, 1)] }), refused, "not an item");
        // A request.
        assert_eq!(ok(Msg::TradeRequest { nonce, item: 0x100, count: 1 }), Verdict::Accept);
        assert_eq!(ok(Msg::TradeRequest { nonce, item: 0x100, count: 0 }), refused);
        assert_eq!(ok(Msg::TradeRequest { nonce, item: 0x9999, count: 1 }), refused);
        // A price: 0 declines, over PRICE_MAX is refused.
        assert_eq!(ok(Msg::TradePrice { nonce, item: 0x100, each: 0 }), Verdict::Accept);
        assert_eq!(ok(Msg::TradePrice { nonce, item: 0x100, each: PRICE_MAX }), Verdict::Accept);
        assert_eq!(ok(Msg::TradePrice { nonce, item: 0x100, each: PRICE_MAX + 1 }), refused);
        assert_eq!(ok(Msg::TradePrice { nonce, item: 0x9999, each: 5 }), refused);
        // A buy, a sale and a refusal: count, price, total and item.
        for make in [
            (|nonce, item, count, each| Msg::TradeBuy { nonce, item, count, each }) as fn(u16, u32, u16, u32) -> Msg,
            |nonce, item, count, each| Msg::TradeSold { nonce, item, count, each },
            |nonce, item, count, each| Msg::TradeRefused { nonce, item, count, each },
        ] {
            assert_eq!(ok(make(nonce, 0x100, 3, 40)), Verdict::Accept);
            assert_eq!(ok(make(nonce, 0x100, 0, 40)), refused, "no count");
            assert_eq!(ok(make(nonce, 0x100, 3, 0)), refused, "no price");
            assert_eq!(ok(make(nonce, 0x100, 3, PRICE_MAX + 1)), refused, "over the price cap");
            assert_eq!(ok(make(nonce, 0x100, u16::MAX, PRICE_MAX)), refused, "a total past i32::MAX");
            assert_eq!(ok(make(nonce, 0x100, 21_475, PRICE_MAX)), Verdict::Accept, "just under it");
            assert_eq!(ok(make(nonce, 0x100, 21_476, PRICE_MAX)), refused, "just over it");
            assert_eq!(ok(make(nonce, 0x9999, 3, 40)), refused, "not an item");
        }
    }

    #[test]
    fn a_talk_names_a_placed_actor() {
        let acts = host_rules(Open::default());
        let now = Instant::now();
        // Each a few seconds on from the last: this is the rule's test, not the rate's.
        let n = std::cell::Cell::new(0u64);
        let talk = |actor: u32| {
            n.set(n.get() + 1);
            check_forms(&acts, 2, game::TALK, 0, actor.to_le_bytes().to_vec(), now + Duration::from_secs(5 * n.get()))
        };
        assert_eq!(talk(0x0010_C1DB), Verdict::Accept, "a placed actor");
        // A companion or a scheduled NPC is far from its place in the plugins: its live place
        // is not the table's, so the guest is not refused for it.
        assert_eq!(talk(0x0010_C1DC), Verdict::Accept, "an actor far from where the guest stood");
        assert_eq!(talk(0), Verdict::Accept, "the talk is over");
        assert_eq!(talk(0xDEAD_BEEF), Verdict::Drop(Reason::Rule("talk: not a placed actor")), "a reference the host does not know");
        // Made at run time: not in the plugins, refused, and not counted as the guest's fault.
        assert_eq!(talk(0xFF00_0123), Verdict::Drop(Reason::Unknown), "a spawned actor");
    }

    #[test]
    fn an_unknown_reference_is_not_a_bad_message() {
        let mut g = nvf_coop::guard::Guard::default();
        g.note(nvf_coop::guard::Kind::State, Reason::Unknown, Some(1));
        assert_eq!(g.bad(1), 0);
        g.note(nvf_coop::guard::Kind::State, Reason::Bounds, Some(1));
        assert_eq!(g.bad(1), 1);
    }

    #[test]
    fn a_duels_countdown_winner_and_throw_are_bounded() {
        let open = Open::default();
        let acts = host_rules(open.clone());
        let now = Instant::now();
        let nonce = 1 << 12 | 3;
        open.invited(nonce, 1, 2);
        let freeze = |delay_ms| Msg::DuelFreeze { nonce, delay_ms }.encode();
        assert_eq!(check(&acts, 1, game::SOCIAL, 2, freeze(DELAY_MS.0), now), Verdict::Accept);
        assert_eq!(check(&acts, 1, game::SOCIAL, 2, freeze(DELAY_MS.1 + 1), now), Verdict::Drop(Reason::Bounds));
        assert_eq!(check(&acts, 1, game::SOCIAL, 2, freeze(0), now), Verdict::Drop(Reason::Bounds));
        let over = |winner| Msg::DuelOver { nonce, winner }.encode();
        for winner in [1, 2, 255] {
            assert_eq!(check(&acts, 1, game::SOCIAL, 2, over(winner), now), Verdict::Accept, "winner {winner}");
        }
        assert_eq!(check(&acts, 1, game::SOCIAL, 2, over(3), now), Verdict::Drop(Reason::Bounds), "not in the duel");
        // A thrown body goes to everyone: any table but the sender, of a duel it is in.
        let thrown = |velocity| Msg::Thrown { nonce, velocity }.encode();
        assert_eq!(check(&acts, 2, game::SOCIAL, 0, thrown([-226, 226, 180]), now), Verdict::Accept);
        assert_eq!(check(&acts, 2, game::SOCIAL, 0, thrown([0, 0, 50_000]), now), Verdict::Drop(Reason::Bounds), "faster than a duel throws");
        assert_eq!(check(&acts, 3, game::SOCIAL, 0, thrown([0, 0, 10]), now), NO_SUCH, "not in the duel");
    }

    #[test]
    fn a_guests_emote_is_one_of_the_catalog() {
        let state = |id, phase| {
            let mut state = PlayerState::at(Place::Exterior(0xDA726), [0.0; 3], 0.0);
            state.emote = EmoteState { id, phase, ..EmoteState::default() };
            nvf_coop::net::Msg::State { id: 1, seq: 1, state }
        };
        let ctx = Ctx { from_id: Some(1), kind: nvf_coop::guard::Kind::State, now: Instant::now(), last: None, forms: None };
        let n = nvf_play::emotes::CATALOG.len() as u8;
        for (id, phase, want) in [(0, 0, Verdict::Accept), (n, nvf_coop::net::emote::ONCE, Verdict::Accept), (n + 1, 0, Verdict::Drop(Reason::Bounds)), (1, 9, Verdict::Drop(Reason::Bounds))] {
            assert_eq!(EmoteBounds.check(&ctx, &Subject::Msg(&state(id, phase))), want, "emote {id}, phase {phase}");
        }
    }

    #[test]
    fn pings_and_talks_are_checked_and_kept_to_their_rates() {
        let acts = host_rules(Open::default());
        let now = Instant::now();
        let ping = PingMsg { world: 0xDA726, interior: 0, pos: [1.0, 2.0, 3.0], target: 0, clear: false, actor: false }.encode();
        assert_eq!(check(&acts, 1, game::PING, 0, ping.clone(), now), Verdict::Drop(Reason::Rule("ping: not for everyone")));
        for _ in 0..3 {
            assert_eq!(check(&acts, 1, game::PING, game::EVERYONE, ping.clone(), now), Verdict::Accept);
        }
        assert_eq!(check(&acts, 1, game::PING, game::EVERYONE, ping.clone(), now), Verdict::Drop(Reason::Rate), "over the burst");
        assert_eq!(check(&acts, 1, game::PING, game::EVERYONE, ping, now + Duration::from_secs(2)), Verdict::Accept, "refilled");
        assert_eq!(check(&acts, 2, game::TALK, 0, 0x0010_C1DBu32.to_le_bytes().to_vec(), now), Verdict::Accept);
        assert_eq!(check(&acts, 2, game::TALK, 0, vec![1, 2, 3], now), Verdict::Drop(Reason::Rule("talk: not one reference")));
        assert_eq!(check(&acts, 2, game::TALK, 5, 0u32.to_le_bytes().to_vec(), now), Verdict::Drop(Reason::Rule("talk: not one reference")));
        assert_eq!(check(&acts, 2, 99, 0, Vec::new(), now), Verdict::Drop(Reason::Rule("action of no known game")));
    }
}
