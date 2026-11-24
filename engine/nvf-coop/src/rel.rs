//! A small reliable, ordered stream over the co-op socket (docs/design/coop-replication.md
//! §6.2): each side numbers the items it sends, the other says how far it has them all
//! (`ack`: the next number it waits for), and anything not acknowledged after [`RESEND`]
//! goes again. Go-back-N: a packet carries a run of consecutive items, and a receiver
//! takes only the run that starts at or before the item it waits for, so items arrive
//! once and in order. One stream per host-guest pair, both ways ([`crate::net`]).
//!
//! An item is the replication layer's own bytes (`crate::repl`) and must fit a packet
//! ([`ITEM_MAX`]). Nothing is ever dropped from the queue; a stream is only started
//! again (`epoch`, kept by `net`) when a guest says Hello again, and then the layer
//! above sends its whole state again.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Unacknowledged items go again after this long (a LAN round trip is far shorter; a
/// home connection's is under 100 ms).
pub const RESEND: Duration = Duration::from_millis(150);
/// The most one item takes.
pub const ITEM_MAX: usize = 1100;
/// The most item bytes one packet carries.
pub const PACKET_BUDGET: usize = 1150;
/// An acknowledgement alone goes at most this often when there is nothing to send.
pub const ACK_EVERY: Duration = Duration::from_millis(30);

#[derive(Default)]
pub struct Reliable {
    /// The number of the next item queued.
    next_seq: u32,
    /// Items not yet acknowledged, oldest first: number, bytes, when last sent.
    unacked: VecDeque<(u32, Vec<u8>, Option<Instant>)>,
    /// The next item number this side waits for.
    expected: u32,
    /// Something came since the last acknowledgement went.
    ack_due: bool,
    last_ack: Option<Instant>,
    /// Items sent again, for the perf line.
    pub resent: u64,
}

/// What one packet of the stream carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub ack: u32,
    pub first: u32,
    pub items: Vec<Vec<u8>>,
}

impl Reliable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue an item (cut to [`ITEM_MAX`]: the layer above keeps under it).
    pub fn push(&mut self, mut item: Vec<u8>) {
        item.truncate(ITEM_MAX);
        self.unacked.push_back((self.next_seq, item, None));
        self.next_seq = self.next_seq.wrapping_add(1);
    }

    /// How many items wait for an acknowledgement.
    pub fn pending(&self) -> usize {
        self.unacked.len()
    }

    /// The other side has every item before `ack`.
    pub fn acked(&mut self, ack: u32) {
        // An acknowledgement of items never sent is from an older stream: ignored.
        let ahead = ack.wrapping_sub(self.unacked.front().map_or(self.next_seq, |u| u.0));
        if ahead > self.unacked.len() as u32 {
            return;
        }
        while self.unacked.front().is_some_and(|u| u.0.wrapping_sub(ack) > u32::MAX / 2) {
            self.unacked.pop_front();
        }
    }

    /// A frame came: its acknowledgement, and the items new to this side, in order.
    pub fn receive(&mut self, frame: Frame) -> Vec<Vec<u8>> {
        self.acked(frame.ack);
        if frame.items.is_empty() {
            return Vec::new();
        }
        self.ack_due = true;
        let skip = self.expected.wrapping_sub(frame.first);
        // A run starting after the one awaited (an item lost on the way): dropped, the
        // sender goes back to it.
        if skip > u32::MAX / 2 || skip as usize >= frame.items.len() {
            return Vec::new();
        }
        let new: Vec<Vec<u8>> = frame.items.into_iter().skip(skip as usize).collect();
        self.expected = self.expected.wrapping_add(new.len() as u32);
        new
    }

    /// The frames to send now: items never sent or not acknowledged after [`RESEND`],
    /// as runs of at most [`PACKET_BUDGET`] bytes (at most `max_packets`); or an
    /// acknowledgement alone when one is due.
    pub fn frames(&mut self, now: Instant, max_packets: usize) -> Vec<Frame> {
        let mut out = Vec::new();
        let due = |sent: Option<Instant>| sent.is_none_or(|t| now.duration_since(t) >= RESEND);
        let mut i = match self.unacked.iter().position(|u| due(u.2)) {
            Some(i) => i,
            None => self.unacked.len(),
        };
        while i < self.unacked.len() && out.len() < max_packets {
            let first = self.unacked[i].0;
            let mut items = Vec::new();
            let mut bytes = 0;
            while i < self.unacked.len() && (items.is_empty() || bytes + self.unacked[i].1.len() + 2 <= PACKET_BUDGET) {
                let (_, item, sent) = &mut self.unacked[i];
                if sent.is_some() {
                    self.resent += 1;
                }
                *sent = Some(now);
                bytes += item.len() + 2;
                items.push(item.clone());
                i += 1;
            }
            out.push(Frame { ack: self.expected, first, items });
        }
        if out.is_empty() && self.ack_due && self.last_ack.is_none_or(|t| now.duration_since(t) >= ACK_EVERY) {
            out.push(Frame { ack: self.expected, first: self.next_seq, items: Vec::new() });
        }
        if !out.is_empty() {
            self.ack_due = false;
            self.last_ack = Some(now);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run two streams against each other through `wire`, which may drop or reorder.
    fn exchange(a: &mut Reliable, b: &mut Reliable, now: Instant, mut wire: impl FnMut(usize, Frame) -> Option<Frame>) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
        let (mut at_a, mut at_b) = (Vec::new(), Vec::new());
        for (n, f) in a.frames(now, 8).into_iter().enumerate() {
            if let Some(f) = wire(n, f) {
                at_b.extend(b.receive(f));
            }
        }
        for (n, f) in b.frames(now, 8).into_iter().enumerate() {
            if let Some(f) = wire(100 + n, f) {
                at_a.extend(a.receive(f));
            }
        }
        (at_a, at_b)
    }

    #[test]
    fn items_arrive_once_and_in_order_through_loss() {
        let (mut a, mut b) = (Reliable::new(), Reliable::new());
        for i in 0..300u32 {
            a.push(i.to_le_bytes().repeat(40));
        }
        let mut got = Vec::new();
        let mut now = Instant::now();
        let mut round = 0usize;
        // Every third packet each way is lost.
        while got.len() < 300 && round < 200 {
            let (_, at_b) = exchange(&mut a, &mut b, now, |n, f| (!(n + round).is_multiple_of(3)).then_some(f));
            got.extend(at_b);
            now += RESEND;
            round += 1;
        }
        assert_eq!(got.len(), 300, "after {round} rounds");
        for (i, item) in got.iter().enumerate() {
            assert_eq!(&item[..4], &(i as u32).to_le_bytes());
        }
        // Once all is acknowledged nothing waits.
        for _ in 0..4 {
            exchange(&mut a, &mut b, now, |_, f| Some(f));
            now += RESEND;
        }
        assert_eq!(a.pending(), 0);
        assert!(a.resent > 0);
    }

    #[test]
    fn a_late_or_doubled_frame_changes_nothing() {
        let (mut a, mut b) = (Reliable::new(), Reliable::new());
        a.push(vec![1]);
        a.push(vec![2]);
        let now = Instant::now();
        let frames = a.frames(now, 8);
        assert_eq!(frames.len(), 1);
        assert_eq!(b.receive(frames[0].clone()), vec![vec![1], vec![2]]);
        assert!(b.receive(frames[0].clone()).is_empty());
        a.push(vec![3]);
        let later = a.frames(now, 8);
        // Only the new item goes (the first two are not due again yet).
        assert_eq!(later[0].first, 2);
        assert_eq!(b.receive(later[0].clone()), vec![vec![3]]);
        // An acknowledgement from another stream (beyond what was sent) is ignored.
        a.acked(1000);
        assert_eq!(a.pending(), 3);
        let ack = b.frames(now, 8);
        a.receive(ack[0].clone());
        assert_eq!(a.pending(), 0);
    }

    #[test]
    fn nothing_goes_when_nothing_is_due() {
        let mut a = Reliable::new();
        assert!(a.frames(Instant::now(), 8).is_empty());
        a.push(vec![9; 600]);
        a.push(vec![9; 600]);
        // Two big items do not fit one packet.
        let f = a.frames(Instant::now(), 8);
        assert_eq!(f.len(), 2);
        assert!(a.frames(Instant::now(), 8).is_empty());
    }
}
