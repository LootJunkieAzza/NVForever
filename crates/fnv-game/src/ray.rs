//! Ray casts through the engine's physics for the functions that ask for them (JIP's
//! GetP2PRayCastRange): the engine lends a caster for the length of a script tick
//! ([`with_caster`]); without one (headless) a cast hits nothing.

use std::cell::Cell;

/// A cast from one game-space point toward another: the distance to the first hit along
/// the segment, `None` for no hit.
pub type Cast<'a> = dyn Fn([f32; 3], [f32; 3]) -> Option<f32> + 'a;

thread_local! {
    static CASTER: Cell<Option<*const Cast<'static>>> = const { Cell::new(None) };
}

/// Run `body` with `caster` answering [`cast`] on this thread.
pub fn with_caster<R>(caster: &Cast<'_>, body: impl FnOnce() -> R) -> R {
    struct Restore(Option<*const Cast<'static>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CASTER.with(|c| c.set(self.0));
        }
    }
    // SAFETY: the pointer is only read by `cast` while `body` runs on this thread, and
    // `Restore` takes it back out when `body` returns or unwinds, so it never outlives
    // the borrow of `caster`.
    let p = unsafe { std::mem::transmute::<*const Cast<'_>, *const Cast<'static>>(caster as *const Cast<'_>) };
    let _restore = Restore(CASTER.with(|c| c.replace(Some(p))));
    body()
}

/// The engine's answer for a cast from `from` to `to`; `None` without a caster.
pub fn cast(from: [f32; 3], to: [f32; 3]) -> Option<f32> {
    CASTER.with(|c| c.get()).and_then(|p| {
        // SAFETY: set by `with_caster`, whose borrow is live while the pointer is set.
        let f = unsafe { &*p };
        f(from, to)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lent_caster_answers_only_while_lent() {
        assert_eq!(cast([0.0; 3], [0.0, 10.0, 0.0]), None);
        let wall = 4.0f32;
        let f = move |a: [f32; 3], b: [f32; 3]| (b[1] - a[1] > wall).then_some(wall);
        let inside = with_caster(&f, || cast([0.0; 3], [0.0, 10.0, 0.0]));
        assert_eq!(inside, Some(4.0));
        assert_eq!(cast([0.0; 3], [0.0, 10.0, 0.0]), None);
    }
}
