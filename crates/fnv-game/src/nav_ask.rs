//! Questions on the navmeshes for the functions that ask them (ITR's GetPathLength,
//! IsPointOnNavmesh, GetCoverPointsInRadius and their kin, `crate::nvse_itr2`): the
//! engine, which holds the nav graph, lends an answerer for the length of a script tick
//! ([`with_nav`]), as it lends its ray caster (`crate::ray`); without one (headless) every
//! question has no answer: no path, no navmesh, no cover.

use std::cell::Cell;

/// What the engine answers.
pub trait NavAsk {
    /// A complete path on the navmeshes from one game-space point to another, its points
    /// in order from `from` to `to` (both included); `None` when there is none.
    fn path(&self, from: [f32; 3], to: [f32; 3]) -> Option<Vec<[f32; 3]>>;
    /// Whether a point lies on a loaded navmesh triangle; `near` is a reference whose
    /// cell the point is looked for in.
    fn on_navmesh(&self, at: [f32; 3], near: Option<u32>) -> bool;
    /// The cover points of the loaded navmeshes within `radius` of `at`: each its place
    /// and its flags (bits 0-3 the height in 16-unit steps, 4 left open, 5 right open, 6
    /// the edge slot) [documented: GetCoverPointsInRadius].
    fn cover(&self, at: [f32; 3], radius: f32) -> Vec<([f32; 3], u32)>;
    /// The centre of a loaded navmesh triangle: the one whose area holds `at` seen from
    /// above (`under`), else the nearest; `skip_disabled` leaves out disabled
    /// triangles and `z_limit` those further than it up or down (JohnnyGuitar's
    /// GetPointInNavMesh and GetNearestNavMeshTriangle, `crate::nvse_jg10`). None by
    /// default: an answerer that does not know triangles.
    fn triangle(&self, _at: [f32; 3], _under: bool, _skip_disabled: bool, _z_limit: Option<f32>) -> Option<[f32; 3]> {
        None
    }
}

thread_local! {
    static NAV: Cell<Option<*const (dyn NavAsk + 'static)>> = const { Cell::new(None) };
}

/// Run `body` with `nav` answering on this thread.
pub fn with_nav<R>(nav: &dyn NavAsk, body: impl FnOnce() -> R) -> R {
    // SAFETY: the pointer is only read by `ask` while `body` runs on this thread (or on a
    // thread a nested script run moved to while this one waits, `Game::nested`), and
    // `lend` takes it back out when `body` returns or unwinds, so it never outlives the
    // borrow of `nav`.
    let p = unsafe { std::mem::transmute::<*const (dyn NavAsk + '_), *const (dyn NavAsk + 'static)>(nav as *const dyn NavAsk) };
    unsafe { lend(Some(p), body) }
}

/// The answerer lent on this thread now, to carry to the thread a nested script run
/// moves to (`Game::nested`).
pub(crate) fn lent() -> Option<*const (dyn NavAsk + 'static)> {
    NAV.with(|c| c.get())
}

/// Run `body` with `p` (from [`lent`] or [`with_nav`]) answering on this thread, the one
/// before put back when it returns or unwinds.
///
/// # Safety
/// The answerer `p` points to must stay alive, and be used by no other thread, while
/// `body` runs.
pub(crate) unsafe fn lend<R>(p: Option<*const (dyn NavAsk + 'static)>, body: impl FnOnce() -> R) -> R {
    struct Restore(Option<*const (dyn NavAsk + 'static)>);
    impl Drop for Restore {
        fn drop(&mut self) {
            NAV.with(|c| c.set(self.0));
        }
    }
    let _restore = Restore(NAV.with(|c| c.replace(p)));
    body()
}

/// The lent answerer's answer, `None` without one.
fn ask<R>(f: impl FnOnce(&dyn NavAsk) -> R) -> Option<R> {
    NAV.with(|c| c.get()).map(|p| {
        // SAFETY: set by `with_nav`, whose borrow is live while the pointer is set.
        let n = unsafe { &*p };
        f(n)
    })
}

/// [`NavAsk::path`]; `None` without an answerer.
pub fn path(from: [f32; 3], to: [f32; 3]) -> Option<Vec<[f32; 3]>> {
    ask(|n| n.path(from, to)).flatten()
}

/// [`NavAsk::on_navmesh`]; false without an answerer.
pub fn on_navmesh(at: [f32; 3], near: Option<u32>) -> bool {
    ask(|n| n.on_navmesh(at, near)).unwrap_or(false)
}

/// [`NavAsk::triangle`]; none without an answerer.
pub fn triangle(at: [f32; 3], under: bool, skip_disabled: bool, z_limit: Option<f32>) -> Option<[f32; 3]> {
    ask(|n| n.triangle(at, under, skip_disabled, z_limit)).flatten()
}

/// [`NavAsk::cover`]; none without an answerer.
pub fn cover(at: [f32; 3], radius: f32) -> Vec<([f32; 3], u32)> {
    ask(|n| n.cover(at, radius)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Line;
    impl NavAsk for Line {
        fn path(&self, from: [f32; 3], to: [f32; 3]) -> Option<Vec<[f32; 3]>> {
            Some(vec![from, to])
        }
        fn on_navmesh(&self, at: [f32; 3], _near: Option<u32>) -> bool {
            at[2] == 0.0
        }
        fn cover(&self, _at: [f32; 3], _radius: f32) -> Vec<([f32; 3], u32)> {
            Vec::new()
        }
    }

    #[test]
    fn a_lent_answerer_answers_only_while_lent() {
        assert_eq!(path([0.0; 3], [1.0, 0.0, 0.0]), None);
        assert!(with_nav(&Line, || on_navmesh([0.0; 3], None)));
        assert_eq!(with_nav(&Line, || path([0.0; 3], [1.0, 0.0, 0.0])), Some(vec![[0.0; 3], [1.0, 0.0, 0.0]]));
        assert!(!on_navmesh([0.0; 3], None));
    }
}
