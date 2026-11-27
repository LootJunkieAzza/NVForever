//! What the players' games send each other in co-op beyond their states: actions, short
//! messages one game sends for the others' (an invitation, a duel's signal, a trade's
//! offer, a ping). nvforever `coop.rs` carries them on the session's reliable stream
//! (`nvf_coop::action`: each once, in order, the host passing a guest's on to the other
//! guests after its checks) from [`Tables::actions_out`], and fills
//! [`Tables::actions_in`] with the others'.
//!
//! An action names its layout with a `game` byte ([`game`]) and whom or what it is for
//! with a `table` (a player's id, or [`game::EVERYONE`]); every player's game gets every
//! action and takes the ones for it. Every blob is little-endian, its game's own layout
//! ([`W`], [`R`]); one that does not parse is ignored.

use fnv_util::words::{tr, trf};
use bevy::prelude::*;
use std::collections::HashMap;

/// The layouts of the actions, by their `game` byte (never 0).
pub mod game {
    /// The interaction wheel's messages between two players (`crate::coop_social::wire`;
    /// their table the player they are for).
    pub const SOCIAL: u8 = 7;
    /// A ping (`crate::world_marks`; table [`EVERYONE`]).
    pub const PING: u8 = 10;
    /// The actor a player is in a conversation with, 0 for none (`crate::coop_talk`;
    /// table 0).
    pub const TALK: u8 = 40;
    /// The table of an action for every player.
    pub const EVERYONE: u32 = u32::MAX;
}

/// The co-op side of the actions, kept by nvforever `coop.rs`.
#[derive(Resource, Default, Debug)]
pub struct Tables {
    /// A co-op session is on.
    pub on: bool,
    /// This player's id in it.
    pub me: Option<u8>,
    /// This game hosts the session.
    pub host: bool,
    pub names: HashMap<u8, String>,
    /// Actions to send: table, game, blob.
    pub actions_out: Vec<(u32, u8, Vec<u8>)>,
    /// Actions from the others: from, table, game, blob.
    pub actions_in: Vec<(u8, u32, u8, Vec<u8>)>,
}

impl Tables {
    /// An action for the others (nothing outside a session).
    pub fn send(&mut self, table: u32, game: u8, blob: Vec<u8>) {
        if self.on {
            self.actions_out.push((table, game, blob));
        }
    }

    /// The actions of `game` heard since the last call: from, table, blob.
    pub fn take_actions(&mut self, game: u8) -> Vec<(u8, u32, Vec<u8>)> {
        let mut out = Vec::new();
        self.actions_in.retain(|(from, table, g, blob)| {
            if *g == game {
                out.push((*from, *table, blob.clone()));
                false
            } else {
                true
            }
        });
        out
    }

    pub fn name(&self, player: u8) -> String {
        self.names.get(&player).filter(|n| !n.is_empty()).cloned().unwrap_or_else(|| if player == 0 { tr("Host").into() } else { trf("Player {n}", &[("n", &player)]) })
    }
}

// ---- the blobs ----------------------------------------------------------------------

#[derive(Default)]
pub struct W(pub Vec<u8>);

impl W {
    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }
    pub fn u16(&mut self, v: u16) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn f32(&mut self, v: f32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn bytes(&mut self, v: &[u8]) -> &mut Self {
        self.u8(v.len().min(255) as u8);
        self.0.extend_from_slice(&v[..v.len().min(255)]);
        self
    }
    pub fn str(&mut self, s: &str) -> &mut Self {
        self.bytes(s.as_bytes())
    }
}

pub struct R<'a>(pub &'a [u8]);

impl<'a> R<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.0.len() < n {
            return None;
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Some(a)
    }
    pub fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }
    pub fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }
    pub fn u32(&mut self) -> Option<u32> {
        self.take(4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    /// A float, finite only.
    pub fn f32(&mut self) -> Option<f32> {
        self.take(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).filter(|v| v.is_finite())
    }
    pub fn bytes(&mut self) -> Option<&'a [u8]> {
        let n = self.u8()? as usize;
        self.take(n)
    }
    pub fn str(&mut self) -> Option<String> {
        self.bytes().map(|b| String::from_utf8_lossy(b).into_owned())
    }
    pub fn done(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_go_out_only_in_a_session_and_come_in_by_game() {
        let mut t = Tables::default();
        t.send(1, game::SOCIAL, vec![1]);
        assert!(t.actions_out.is_empty(), "no session");
        t.on = true;
        t.send(1, game::SOCIAL, vec![1]);
        assert_eq!(t.actions_out, vec![(1, game::SOCIAL, vec![1])]);
        t.actions_in = vec![(2, 0, game::PING, vec![9]), (3, 1, game::SOCIAL, vec![8])];
        assert_eq!(t.take_actions(game::SOCIAL), vec![(3, 1, vec![8])]);
        assert_eq!(t.actions_in, vec![(2, 0, game::PING, vec![9])], "the others stay");
    }

    #[test]
    fn a_blob_reads_back_and_refuses_what_is_short_or_not_finite() {
        let mut w = W::default();
        w.u8(1).u16(2).u32(3).f32(4.5).str("Doc");
        let mut r = R(&w.0);
        assert_eq!((r.u8(), r.u16(), r.u32(), r.f32(), r.str()), (Some(1), Some(2), Some(3), Some(4.5), Some("Doc".into())));
        assert!(r.done());
        assert_eq!(R(&f32::NAN.to_le_bytes()).f32(), None);
        assert_eq!(R(&[1, 0]).u32(), None);
    }
}
