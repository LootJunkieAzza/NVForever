//! The loader: prepares requested cells on its own thread and hands them back in the
//! order they were requested, whatever order they finish in.
//!
//! The order matters: rapier's results depend on the order bodies are inserted
//! (docs/spec-notes/physics.md §5), so the main thread inserts only the front of the
//! queue, once it is ready, and a cell's references go in by FormID (the job lists them
//! so). A dedicated thread rather than Bevy's task pools: the render side's LOD bakes
//! keep those busy for seconds at a time, and physics cells must not wait behind them.

use super::cells::PhysCell;
use super::prepare::Prepared;
use std::collections::{HashSet, VecDeque};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};

/// The work of preparing one cell.
pub type Work = Box<dyn FnOnce() -> Prepared + Send>;

struct Request {
    ticket: u64,
    work: Work,
}

struct Done {
    ticket: u64,
    prepared: Prepared,
}

struct Pending {
    cell: PhysCell,
    ticket: u64,
    result: Option<Prepared>,
}

struct Worker {
    tx: Sender<Request>,
    rx: Receiver<Done>,
}

// The channel ends and the held jobs are not `Sync`; the mutexes make the loader so (a
// Bevy resource must be), and `&mut self` reaches them through `get_mut` without locking.
#[derive(Default)]
pub struct Loader {
    worker: Option<Mutex<Worker>>,
    /// The thread is gone (it panicked): everything loads inline from then on.
    dead: bool,
    /// Tickets cancelled before the thread started them.
    cancelled: Arc<Mutex<HashSet<u64>>>,
    next_ticket: u64,
    /// Requested cells in request order.
    queue: VecDeque<Pending>,
    /// Test aid: jobs held here instead of going to the thread ([`Loader::manual`]).
    manual: Option<Mutex<Vec<Request>>>,
}

impl Loader {
    /// A loader whose jobs run only when [`Loader::run_manual`] says so (tests).
    #[cfg(test)]
    pub fn manual() -> Loader {
        Loader { manual: Some(Mutex::default()), ..Default::default() }
    }

    /// The requested cells, in request order.
    pub fn pending(&self) -> impl Iterator<Item = PhysCell> + '_ {
        self.queue.iter().map(|p| p.cell)
    }

    /// Queue `work` for `cell`. Returns false when there is no thread to run it (the
    /// caller then prepares the cell itself).
    pub fn request(&mut self, cell: PhysCell, work: Work) -> bool {
        if self.dead {
            return false;
        }
        let ticket = self.next_ticket;
        self.next_ticket += 1;
        let req = Request { ticket, work };
        if let Some(m) = self.manual.as_mut() {
            unpoisoned(m).push(req);
        } else {
            let w = self.worker.get_or_insert_with(|| Mutex::new(spawn(self.cancelled.clone())));
            if unpoisoned(w).tx.send(req).is_err() {
                self.died();
                return false;
            }
        }
        self.queue.push_back(Pending { cell, ticket, result: None });
        true
    }

    /// Forget a requested cell. Returns whether it was pending.
    pub fn cancel(&mut self, cell: PhysCell) -> bool {
        let Some(i) = self.queue.iter().position(|p| p.cell == cell) else { return false };
        let p = self.queue.remove(i).expect("index from position");
        if p.result.is_none() {
            self.cancelled.lock().unwrap_or_else(|e| e.into_inner()).insert(p.ticket);
        }
        true
    }

    /// Forget every requested cell.
    pub fn clear(&mut self) {
        let mut c = self.cancelled.lock().unwrap_or_else(|e| e.into_inner());
        c.extend(self.queue.drain(..).filter(|p| p.result.is_none()).map(|p| p.ticket));
    }

    /// Collect what the thread finished.
    pub fn poll(&mut self) {
        loop {
            let Some(w) = self.worker.as_mut() else { return };
            match unpoisoned(w).rx.try_recv() {
                Ok(done) => self.deliver(done),
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.died();
                    return;
                }
            }
        }
    }

    /// Wait until every requested cell is ready (or the thread is gone).
    #[cfg(test)]
    pub fn wait_all(&mut self) {
        while self.queue.iter().any(|p| p.result.is_none()) {
            let Some(w) = self.worker.as_mut() else { return };
            match unpoisoned(w).rx.recv() {
                Ok(done) => self.deliver(done),
                Err(_) => {
                    self.died();
                    return;
                }
            }
        }
    }

    fn deliver(&mut self, done: Done) {
        self.cancelled.lock().unwrap_or_else(|e| e.into_inner()).remove(&done.ticket);
        // A cancelled cell's ticket is no longer queued: its result is dropped.
        if let Some(p) = self.queue.iter_mut().find(|p| p.ticket == done.ticket) {
            p.result = Some(done.prepared);
        }
    }

    fn died(&mut self) {
        bevy::log::error!("the physics loader thread stopped: cells load on the main thread from now on");
        self.worker = None;
        self.dead = true;
        self.queue.clear();
    }

    /// The first requested cell, once it is ready.
    pub fn pop_ready(&mut self) -> Option<Prepared> {
        if self.queue.front()?.result.is_some() {
            self.queue.pop_front().and_then(|p| p.result)
        } else {
            None
        }
    }

    /// Test aid: run the held jobs, in reverse request order when `reverse`, and deliver
    /// their results as they finish.
    #[cfg(test)]
    pub fn run_manual(&mut self, reverse: bool) {
        let mut jobs = std::mem::take(unpoisoned(self.manual.as_mut().expect("a manual loader")));
        if reverse {
            jobs.reverse();
        }
        for r in jobs {
            let prepared = (r.work)();
            self.deliver(Done { ticket: r.ticket, prepared });
        }
    }
}

fn unpoisoned<T>(m: &mut Mutex<T>) -> &mut T {
    m.get_mut().unwrap_or_else(|e| e.into_inner())
}

fn spawn(cancelled: Arc<Mutex<HashSet<u64>>>) -> Worker {
    let (tx, jobs) = channel::<Request>();
    let (results, rx) = channel::<Done>();
    std::thread::Builder::new()
        .name("nvf-phys-loader".into())
        .spawn(move || {
            // Ends when the loader (the sender) is dropped.
            for r in jobs {
                if cancelled.lock().unwrap_or_else(|e| e.into_inner()).remove(&r.ticket) {
                    continue;
                }
                let prepared = (r.work)();
                if results.send(Done { ticket: r.ticket, prepared }).is_err() {
                    return;
                }
            }
        })
        .expect("spawn the physics loader thread");
    Worker { tx, rx }
}
