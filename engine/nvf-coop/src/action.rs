//! Actions between players: short messages one player's game sends for the others' (an
//! invitation, a duel's signal, a trade's offer, a ping), carried on the reliable stream
//! ([`crate::rel`]) as an item of the kind [`ACTION`], so each comes once and in order.
//! What a blob means is the game side's (nvf-play `coop_tables`); here it is bytes with a
//! `game` byte naming its layout and a `table` naming whom or what it is for.
//!
//! - **A guest's** action goes to the host. The host's guard admits it first
//!   ([`Actions`], a [`Rule`] for [`Kind::Item`] of [`ACTION`]), then the host's game
//!   takes it and passes it on to every other guest ([`relay`]), with the sender's id
//!   stamped on it by the host: the `from` a guest writes is never read.
//! - **The host's** own actions go to every guest as they are.
//!
//! An item: [`ACTION`], the game, the sender's id ([`ANYONE`] from a guest), the table
//! (u32), then the blob, at most [`BLOB_MAX`] bytes. Little-endian.
//!
//! The host's checks ([`Actions`]): the item parses, its blob is within [`BLOB_MAX`], its
//! game is one this build registered ([`Actions::register`]), within that game's rate per
//! guest ([`Rate`]), and then the game's own [`GameRule`] (the game side's, which knows
//! what the blob says).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use crate::guard::{Ctx, Kind, Rate, Reason, Rule, Subject, Verdict};

/// The reliable item kind of an action [design: the first of the social layer's range,
/// 0x40-0x5F; 0x41-0x57 are kept for its later parts, 0x58-0x5F for the conversation's].
pub const ACTION: u8 = 0x40;
/// The most an action's blob takes [design: one stock page of a trade, with room].
pub const BLOB_MAX: usize = 1024;
/// The sender byte of a guest's action: the host stamps the real one.
pub const ANYONE: u8 = 255;
/// The bytes before the blob.
const HEAD: usize = 7;

/// An action as the game side sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Action {
    pub game: u8,
    /// The sender (the host's stamp; [`ANYONE`] in a guest's own item).
    pub from: u8,
    /// Whom or what it is for: the game side's (a player's id, a table's reference).
    pub table: u32,
    pub blob: Vec<u8>,
}

impl Action {
    /// The reliable item; None when the blob is over [`BLOB_MAX`].
    pub fn encode(&self) -> Option<Vec<u8>> {
        if self.blob.len() > BLOB_MAX {
            return None;
        }
        let mut b = Vec::with_capacity(HEAD + self.blob.len());
        b.extend_from_slice(&[ACTION, self.game, self.from]);
        b.extend_from_slice(&self.table.to_le_bytes());
        b.extend_from_slice(&self.blob);
        Some(b)
    }

    /// An item of the kind [`ACTION`] (None: another kind, cut short or too long).
    pub fn decode(item: &[u8]) -> Option<Action> {
        if item.len() < HEAD || item[0] != ACTION || item.len() - HEAD > BLOB_MAX {
            return None;
        }
        let table = u32::from_le_bytes([item[3], item[4], item[5], item[6]]);
        Some(Action { game: item[1], from: item[2], table, blob: item[HEAD..].to_vec() })
    }
}

/// What the host does with guest `from`'s admitted action: the action as the others get
/// it (the sender stamped), to take here and to send to every guest but `from`.
pub fn relay(from: u8, mut action: Action) -> Action {
    action.from = from;
    action
}

/// A game's own check on its actions' blobs, on the host (the game side's).
pub trait GameRule: Send + Sync {
    /// `from` is the guest by its address; `ctx.last` is its last accepted state.
    fn check(&self, ctx: &Ctx, from: u8, table: u32, blob: &[u8]) -> Verdict;
}

/// A rule that takes every blob (a game with only the rate to keep).
pub struct AnyBlob;

impl GameRule for AnyBlob {
    fn check(&self, _: &Ctx, _: u8, _: u32, _: &[u8]) -> Verdict {
        Verdict::Accept
    }
}

struct Bucket {
    tokens: f32,
    at: Instant,
}

/// The host's checks on actions (module notes): add it to the guard for
/// `Kind::Item(ACTION)` ([`Actions::kind`]).
#[derive(Default)]
pub struct Actions {
    games: HashMap<u8, (Rate, Box<dyn GameRule>)>,
    buckets: Mutex<HashMap<(u8, u8), Bucket>>,
}

impl Actions {
    /// The guard kind these checks are for.
    pub fn kind() -> Kind {
        Kind::Item(ACTION)
    }

    /// Actions of `game` are taken, each guest at most `rate`, and judged by `rule`.
    pub fn register(&mut self, game: u8, rate: Rate, rule: Box<dyn GameRule>) {
        self.games.insert(game, (rate, rule));
    }

    fn within_rate(&self, from: u8, game: u8, rate: Rate, now: Instant) -> bool {
        let mut buckets = self.buckets.lock().unwrap_or_else(|e| e.into_inner());
        let b = buckets.entry((from, game)).or_insert(Bucket { tokens: rate.burst, at: now });
        let dt = now.saturating_duration_since(b.at).as_secs_f32();
        b.tokens = (b.tokens + dt * rate.per_sec).min(rate.burst);
        b.at = now;
        if b.tokens >= 1.0 {
            b.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

impl Rule for Actions {
    fn check(&self, ctx: &Ctx, subject: &Subject) -> Verdict {
        let Subject::Item(item) = subject else { return Verdict::Accept };
        let Some(a) = Action::decode(item) else { return Verdict::Drop(Reason::Bounds) };
        let Some(from) = ctx.from_id else { return Verdict::Drop(Reason::Rule("action from no guest")) };
        let Some((rate, rule)) = self.games.get(&a.game) else { return Verdict::Drop(Reason::Rule("action of no known game")) };
        if !self.within_rate(from, a.game, *rate, ctx.now) {
            return Verdict::Drop(Reason::Rate);
        }
        rule.check(ctx, from, a.table, &a.blob)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn ctx(from: Option<u8>, now: Instant) -> Ctx<'static> {
        Ctx { from_id: from, kind: Actions::kind(), now, last: None, forms: None }
    }

    #[test]
    fn an_action_comes_back_whole() {
        let a = Action { game: 7, from: ANYONE, table: 0x0010_C1DB, blob: vec![1, 2, 3] };
        let item = a.encode().unwrap();
        assert_eq!(Action::decode(&item), Some(a.clone()));
        assert_eq!(Action::decode(&item[..HEAD - 1]), None, "cut short");
        let mut other = item.clone();
        other[0] = 1;
        assert_eq!(Action::decode(&other), None, "another kind of item");
        assert_eq!(Action { blob: vec![0; BLOB_MAX + 1], ..a.clone() }.encode(), None, "too long to send");
        assert!(Action { blob: vec![0; BLOB_MAX], ..a }.encode().is_some());
    }

    #[test]
    fn the_kind_is_in_the_social_range_the_guard_lets_through() {
        const { assert!(ACTION >= 0x40 && ACTION <= 0x5F && ACTION <= crate::guard::ITEM_KIND_MAX) };
        // A guard with the rules admits an action of a registered game and refuses one just past the last kind.
        let mut g = crate::guard::Guard::default();
        let mut acts = Actions::default();
        acts.register(7, Rate { burst: 4.0, per_sec: 1.0 }, Box::new(AnyBlob));
        g.add_rule(Actions::kind(), Box::new(acts));
        let now = Instant::now();
        let item = Action { game: 7, from: ANYONE, table: 1, blob: vec![1] }.encode().unwrap();
        assert_eq!(g.admit_item(1, &item, now), Verdict::Accept);
        assert_eq!(g.admit_item(1, &[crate::guard::ITEM_KIND_MAX + 1, 0], now), Verdict::Drop(Reason::Bounds));
    }

    #[test]
    fn the_host_stamps_the_sender() {
        let a = Action { game: 7, from: 3, table: 1, blob: Vec::new() };
        assert_eq!(relay(2, a).from, 2, "a guest's own sender byte is never read");
    }

    struct Short;
    impl GameRule for Short {
        fn check(&self, _: &Ctx, _: u8, _: u32, blob: &[u8]) -> Verdict {
            if blob.len() <= 2 {
                Verdict::Accept
            } else {
                Verdict::Drop(Reason::Rule("long"))
            }
        }
    }

    #[test]
    fn the_host_takes_known_games_within_their_rate_and_rule() {
        let mut acts = Actions::default();
        acts.register(7, Rate { burst: 2.0, per_sec: 1.0 }, Box::new(Short));
        let now = Instant::now();
        let item = |game: u8, blob: Vec<u8>| Action { game, from: ANYONE, table: 1, blob }.encode().unwrap();
        let check = |acts: &Actions, from: Option<u8>, item: &[u8], now: Instant| acts.check(&ctx(from, now), &Subject::Item(item));
        assert_eq!(check(&acts, Some(1), &item(7, vec![1]), now), Verdict::Accept);
        assert_eq!(check(&acts, Some(1), &item(8, vec![1]), now), Verdict::Drop(Reason::Rule("action of no known game")));
        assert_eq!(check(&acts, Some(1), &item(7, vec![1, 2, 3]), now), Verdict::Drop(Reason::Rule("long")), "the game's own rule");
        assert_eq!(check(&acts, Some(1), &item(7, vec![1]), now), Verdict::Drop(Reason::Rate), "over the burst");
        assert_eq!(check(&acts, Some(2), &item(7, vec![1]), now), Verdict::Accept, "each guest has its own rate");
        assert_eq!(check(&acts, Some(1), &item(7, vec![1]), now + Duration::from_secs(1)), Verdict::Accept, "refilled");
        assert_eq!(check(&acts, None, &item(7, vec![1]), now), Verdict::Drop(Reason::Rule("action from no guest")));
        assert_eq!(check(&acts, Some(1), &[ACTION, 7], now), Verdict::Drop(Reason::Bounds), "cut short");
    }
}
