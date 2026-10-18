//! Enable's fade-in and Disable's fade-out (`fnv_game::functions::refs::enable`): a
//! reference a script enables with its FadeIn flag (the default) is drawn from clear to
//! whole, one it disables with FadeOut 1 from whole to clear and only then hidden
//! [documented: GECK wiki Enable and Disable]. The children of an enable parent fade in
//! with it unless their XESP says "Pop In", and always pop out [documented: same pages]
//! (`crate::script_events`).
//!
//! - A [`crate::RefChanged`] with `fade` starts one ([`RefFades::start`], from
//!   `crate::activate`'s visuals sync, which keeps a reference fading out drawn); one
//!   without ends a running one (Enable 0 while it fades out pops it back).
//! - The alpha is [`RefFades::alpha`]: linear over [`FADE_SECONDS`] of game time
//!   [hypothesis: neither the length nor the curve is documented]; the renderer draws it
//!   as it draws SetActorAlpha (nvforever `effect_shaders.rs`), for every mesh drawn for
//!   the reference.
//! - In the simulation the reference is enabled or disabled at once: collision, the
//!   crosshair and the scripts see the new state while it fades.
//!
//! Log: `ref fade: 0x… in|out (<s> s)` when one starts, `ref fade summary: N in, M out`
//! at exit.

use bevy::prelude::*;
use std::collections::HashMap;

use crate::{PlaySet, RefChanged};

/// How long a fade lasts, seconds of game time [hypothesis: about a second, as fades
/// look in the original; not measured].
pub const FADE_SECONDS: f32 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Fade {
    start: f32,
    fade_in: bool,
}

/// The fades running, by reference FormID.
#[derive(Resource, Debug, Default)]
pub struct RefFades {
    fades: HashMap<u32, Fade>,
    ins: u32,
    outs: u32,
}

impl RefFades {
    /// Start a fade of `id` at game time `now` (a new one replaces a running one).
    pub fn start(&mut self, id: u32, fade_in: bool, now: f32) {
        self.fades.insert(id, Fade { start: now, fade_in });
        if fade_in {
            self.ins += 1;
        } else {
            self.outs += 1;
        }
        info!("ref fade: 0x{id:08X} {} ({FADE_SECONDS} s)", if fade_in { "in" } else { "out" });
    }

    /// Stop a running fade of `id` (it is drawn as the simulation has it).
    pub fn cancel(&mut self, id: u32) {
        self.fades.remove(&id);
    }

    /// The alpha `id` is drawn with at `now`, while it fades.
    pub fn alpha(&self, id: u32, now: f32) -> Option<f32> {
        let f = self.fades.get(&id)?;
        let t = ((now - f.start) / FADE_SECONDS).clamp(0.0, 1.0);
        Some(if f.fade_in { t } else { 1.0 - t })
    }

    /// Whether `id` is fading out (drawn although it is gone).
    pub fn fading_out(&self, id: u32) -> bool {
        self.fades.get(&id).is_some_and(|f| !f.fade_in)
    }

    /// Whether any fade runs.
    pub fn is_empty(&self) -> bool {
        self.fades.is_empty()
    }

    /// Drop the fades over at `now`; the references that faded out, to be hidden.
    fn finish(&mut self, now: f32) -> Vec<u32> {
        let mut gone = Vec::new();
        self.fades.retain(|&id, f| {
            let over = now - f.start >= FADE_SECONDS;
            if over && !f.fade_in {
                gone.push(id);
            }
            !over
        });
        gone
    }
}

pub struct RefFadePlugin;

impl Plugin for RefFadePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RefFades>().add_systems(Update, finish.in_set(PlaySet::Sync)).add_systems(Last, report);
    }
}

/// Fades that are over end; a reference that faded out is hidden now (the visuals sync
/// takes the `RefChanged` next frame).
fn finish(time: Res<Time>, mut fades: ResMut<RefFades>, mut changed: MessageWriter<RefChanged>) {
    if fades.is_empty() {
        return;
    }
    for form_id in fades.finish(time.elapsed_secs()) {
        changed.write(RefChanged { form_id, fade: false });
    }
}

fn report(mut exit: MessageReader<AppExit>, fades: Res<RefFades>) {
    if exit.read().next().is_some() {
        info!("ref fade summary: {} in, {} out", fades.ins, fades.outs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fade_runs_from_clear_to_whole_and_back() {
        let mut f = RefFades::default();
        f.start(1, true, 10.0);
        f.start(2, false, 10.0);
        assert_eq!(f.alpha(1, 10.0), Some(0.0));
        assert_eq!(f.alpha(1, 10.0 + FADE_SECONDS / 2.0), Some(0.5));
        assert_eq!(f.alpha(2, 10.0 + FADE_SECONDS / 4.0), Some(0.75));
        assert!(f.fading_out(2) && !f.fading_out(1));
        assert_eq!(f.alpha(3, 10.0), None);
        assert_eq!(f.finish(10.0 + FADE_SECONDS), vec![2], "only the fade-out hides its reference");
        assert!(f.is_empty());
    }

    #[test]
    fn enabling_again_while_it_fades_out_fades_it_back_in() {
        let mut f = RefFades::default();
        f.start(1, false, 0.0);
        f.start(1, true, 0.5);
        assert!(!f.fading_out(1));
        assert!(f.finish(5.0).is_empty());
        f.start(2, false, 0.0);
        f.cancel(2);
        assert_eq!(f.alpha(2, 0.5), None, "a change without a fade ends it");
    }
}
