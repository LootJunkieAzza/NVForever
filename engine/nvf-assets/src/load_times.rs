//! Where the time of loading a model file goes, summed over the run (#3149): a placed
//! object with collision has its NIF read and decoded once to be drawn (model.rs) and
//! once more for its collision (nvf-play's `phys/prepare.rs`), each on its own thread,
//! and nothing said what the second decode costs. Both sides add their times here and
//! the exit's summary line gives the totals, so any run answers it (the smoke scenario
//! `cell-streaming` reads the line).
//!
//! The times are thread time (several workers run at once), not time the player waits.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// One side's totals: files, and nanoseconds per part.
#[derive(Default)]
pub struct Side {
    files: AtomicU64,
    read: AtomicU64,
    parse: AtomicU64,
    decode: AtomicU64,
    whole: AtomicU64,
}

/// The files prepared to be drawn (`model::prepare_unguarded`).
pub static DRAWN: Side = Side::new();
/// The files read for their collision (`phys/prepare.rs` `read_model`).
pub static COLLISION: Side = Side::new();

impl Side {
    const fn new() -> Side {
        Side { files: AtomicU64::new(0), read: AtomicU64::new(0), parse: AtomicU64::new(0), decode: AtomicU64::new(0), whole: AtomicU64::new(0) }
    }

    /// One file: the archive read, `NifFile::parse`, `Decoded::new`, and the whole of
    /// its preparing (those three and what was built from them).
    pub fn add(&self, read: Duration, parse: Duration, decode: Duration, whole: Duration) {
        self.files.fetch_add(1, Ordering::Relaxed);
        for (to, d) in [(&self.read, read), (&self.parse, parse), (&self.decode, decode), (&self.whole, whole)] {
            to.fetch_add(d.as_nanos() as u64, Ordering::Relaxed);
        }
    }

    fn ms(&self) -> (u64, [f64; 4]) {
        let ms = |a: &AtomicU64| a.load(Ordering::Relaxed) as f64 / 1e6;
        (self.files.load(Ordering::Relaxed), [ms(&self.read), ms(&self.parse), ms(&self.decode), ms(&self.whole)])
    }

    fn words(&self) -> String {
        let (files, [read, parse, decode, whole]) = self.ms();
        format!("{files} files, read {read:.0}, parse {parse:.0}, decode {decode:.0}, rest {:.0}, whole {whole:.0} ms", (whole - read - parse - decode).max(0.0))
    }
}

/// Times one file's parts and adds them when dropped, so a file that fails part way is
/// counted with what it did cost.
pub struct Timed {
    side: &'static Side,
    start: Instant,
    last: Instant,
    parts: [Duration; 3],
}

impl Timed {
    pub fn start(side: &'static Side) -> Timed {
        let now = Instant::now();
        Timed { side, start: now, last: now, parts: [Duration::ZERO; 3] }
    }

    fn part(&mut self, i: usize) {
        let now = Instant::now();
        self.parts[i] = now - self.last;
        self.last = now;
    }

    /// The archive read ended.
    pub fn read(&mut self) {
        self.part(0);
    }

    /// `NifFile::parse` ended.
    pub fn parsed(&mut self) {
        self.part(1);
    }

    /// `Decoded::new` ended.
    pub fn decoded(&mut self) {
        self.part(2);
    }
}

impl Drop for Timed {
    fn drop(&mut self) {
        let [read, parse, decode] = self.parts;
        self.side.add(read, parse, decode, self.start.elapsed());
    }
}

/// The exit's line: both sides' totals, and the share of all model loading that is the
/// collision side's own read, parse and decode (the work the drawn side did already).
pub fn summary() -> String {
    summary_of(&DRAWN, &COLLISION)
}

fn summary_of(drawn: &Side, collision: &Side) -> String {
    let (_, [_, _, _, drawn_whole]) = drawn.ms();
    let (_, [read, parse, decode, whole]) = collision.ms();
    let all = drawn_whole + whole;
    let again = read + parse + decode;
    let share = if all > 0.0 { 100.0 * again / all } else { 0.0 };
    format!("model load times (thread time): drawn: {}; collision: {}; collision read+parse+decode is {share:.1}% of both", drawn.words(), collision.words())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_adds_files_and_gives_the_second_decodes_share() {
        let (drawn, collision) = (Side::new(), Side::new());
        let ms = Duration::from_millis;
        drawn.add(ms(1), ms(2), ms(3), ms(60));
        collision.add(ms(1), ms(2), ms(7), ms(20));
        collision.add(ms(0), ms(0), ms(0), ms(20));
        assert_eq!(
            summary_of(&drawn, &collision),
            "model load times (thread time): drawn: 1 files, read 1, parse 2, decode 3, rest 54, whole 60 ms; collision: 2 files, read 1, parse 2, decode 7, rest 30, whole 40 ms; collision read+parse+decode is 10.0% of both"
        );
        assert!(summary_of(&Side::new(), &Side::new()).ends_with("is 0.0% of both"));
        // A file that fails after its read is still counted.
        static SIDE: Side = Side::new();
        {
            let mut t = Timed::start(&SIDE);
            t.read();
        }
        assert_eq!(SIDE.ms().0, 1);
    }
}
