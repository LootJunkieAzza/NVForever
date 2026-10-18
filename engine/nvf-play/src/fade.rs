//! References fading in and out (docs/design/script-function-params.md §2): Enable fades
//! its reference in unless its FadeIn is 0, Disable fades it out when its FadeOut is 1
//! [documented: GECK wiki Enable, Disable], and enable children follow their own rule
//! (`script_events::effects`, which sends [`RefFade`] after the reference's
//! `RefChanged`).
//!
//! - **Look**: every mesh drawn for the reference gets a blended copy of its material
//!   whose alpha goes from 0 to 1 (in) or 1 to 0 (out) in a straight line over
//!   [`FADE_SECONDS`]; at the end the mesh gets its own material back [hypothesis: the
//!   shape and the length; capture job `tools/capture/jobs/enable_fade_timing.json`].
//! - **Out**: the simulation keeps the reference enabled while it fades
//!   (`fnv_game::functions::refs::FADE_OUT_HOLD`, GetDisabled 0 [observed]); its meshes
//!   fade to nothing and stay hidden until the hold ends and the simulation disables it
//!   (`ScriptLooks::disabled_after_fade`, a `RefChanged` from `script`), when collision
//!   goes too. A plain Disable meanwhile hides it at once.
//! - **In**: only a reference present after its change fades in (an enable child "set to
//!   the opposite of its parent" is disabled by its parent's Enable and pops out).
//! - The fade runs on the game's clock (`Time<Virtual>`): it stands still in menus.
//! - A reference whose presence turns back while it fades (Enable 0 during a fade out,
//!   Disable during a fade in) ends its fade at once and is drawn as it now is.
//! - References scripts draw (`script_draw`) are kept while they fade out
//!   ([`Fades::fading_out`]).
//!
//! Log: `fade: <id> in|out over <s> s (<n> meshes)` when one starts (meshes drawn then;
//! a cell loaded later joins in) and `fade: <id> in|out done` at its end.

use crate::script::ScriptState;
use crate::sim::SimState;
use crate::{PlaySet, RefVisual};
use bevy::prelude::*;
use fnv_sim::FormId;
use nvf_assets::NifMaterial;
use std::collections::HashMap;

/// How long a fade lasts, seconds [hypothesis: not measured; capture job
/// `enable_fade_timing`].
pub const FADE_SECONDS: f32 = 1.0;

/// Fade a reference in or out (sent by `script` after the reference's `RefChanged`).
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct RefFade {
    pub form_id: u32,
    pub fade_in: bool,
}

pub struct FadePlugin;

impl Plugin for FadePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<RefFade>().init_resource::<Fades>().add_systems(Update, (start, step).chain().after(crate::activate::sync_visuals).in_set(PlaySet::Sync));
    }
}

/// A fade under way.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Active {
    fade_in: bool,
    /// `Time<Virtual>` seconds when it started.
    since: f32,
}

#[derive(Resource, Default)]
pub struct Fades {
    active: HashMap<u32, Active>,
}

impl Fades {
    /// The reference is fading out: it is still drawn though disabled.
    pub fn fading_out(&self, id: u32) -> bool {
        self.active.get(&id).is_some_and(|a| !a.fade_in)
    }
}

/// On a mesh drawn see-through for a fade: its own material, and the copy drawn.
#[derive(Component)]
struct Fading {
    original: Handle<NifMaterial>,
    copy: Handle<NifMaterial>,
    /// The original's alpha and cut-out, which the fade multiplies.
    alpha: f32,
    cut: Option<f32>,
}

/// The alpha of a fade `t` seconds in (1 at the end of a fade in, 0 at the end of one out).
pub fn fade_alpha(fade_in: bool, t: f32) -> f32 {
    let f = (t / FADE_SECONDS).clamp(0.0, 1.0);
    if fade_in {
        f
    } else {
        1.0 - f
    }
}

/// Whether a reference is (still) on its way out: disabled, or held enabled by a
/// `Disable 1` (`ScriptLooks::fading_out`).
fn going_out(sim: &SimState, script: Option<&ScriptState>, id: u32) -> bool {
    !sim.present(id) || script.and_then(|s| s.rt.as_ref()).is_some_and(|rt| rt.game.script_looks.fading_out.contains_key(&FormId(id)))
}

fn start(mut fades: MessageReader<RefFade>, sim: Res<SimState>, script: Option<Res<ScriptState>>, time: Res<Time<Virtual>>, mut st: ResMut<Fades>, visuals: Query<&RefVisual>) {
    for f in fades.read() {
        if going_out(&sim, script.as_deref(), f.form_id) == f.fade_in {
            continue;
        }
        st.active.insert(f.form_id, Active { fade_in: f.fade_in, since: time.elapsed_secs() });
        let n = visuals.iter().filter(|r| r.0 == f.form_id).count();
        info!("fade: 0x{:08X} {} over {FADE_SECONDS} s ({n} meshes)", f.form_id, if f.fade_in { "in" } else { "out" });
    }
}

#[allow(clippy::type_complexity)]
fn step(
    mut commands: Commands,
    sim: Res<SimState>,
    script: Option<Res<ScriptState>>,
    time: Res<Time<Virtual>>,
    mut st: ResMut<Fades>,
    mut materials: ResMut<Assets<NifMaterial>>,
    mut meshes: Query<(Entity, &RefVisual, &MeshMaterial3d<NifMaterial>, Option<&Fading>, &mut Visibility)>,
) {
    if st.active.is_empty() {
        return;
    }
    let now = time.elapsed_secs();
    // A presence that turned back ends the fade (module notes).
    let out = |id: u32| going_out(&sim, script.as_deref(), id);
    let over = |id: u32, a: &Active| now - a.since >= FADE_SECONDS || out(id) == a.fade_in;
    for (e, rv, mat, fading, mut vis) in &mut meshes {
        let Some(a) = st.active.get(&rv.0) else { continue };
        let t = now - a.since;
        if over(rv.0, a) {
            if let Some(f) = fading {
                commands.entity(e).insert(MeshMaterial3d(f.original.clone())).remove::<Fading>();
                materials.remove(&f.copy);
            }
            vis.set_if_neq(if out(rv.0) { Visibility::Hidden } else { Visibility::Inherited });
            continue;
        }
        if !a.fade_in && !sim.present(rv.0) {
            // Hidden by `sync_visuals`; drawn while it fades out.
            vis.set_if_neq(Visibility::Inherited);
        }
        let alpha = fade_alpha(a.fade_in, t);
        let copy = match fading {
            Some(f) => {
                if let Some(mut m) = materials.get_mut(&f.copy) {
                    m.base.base_color.set_alpha(f.alpha * alpha);
                    if let Some(cut) = f.cut {
                        m.extension.alpha_cut = cut * alpha;
                    }
                }
                continue;
            }
            None => mat.0.clone(),
        };
        let Some(mut m) = materials.get(&copy).cloned() else { continue };
        let base = m.base.base_color.alpha();
        let cut = match m.base.alpha_mode {
            AlphaMode::Mask(c) => Some(c),
            _ => None,
        };
        // A cut-out keeps its cut while it blends.
        if let Some(c) = cut {
            m.extension.alpha_cut = c * alpha;
        }
        if matches!(m.base.alpha_mode, AlphaMode::Opaque | AlphaMode::Mask(_)) {
            m.base.alpha_mode = AlphaMode::Blend;
        }
        m.base.base_color.set_alpha(base * alpha);
        let h = materials.add(m);
        commands.entity(e).insert((MeshMaterial3d(h.clone()), Fading { original: copy, copy: h, alpha: base, cut }));
    }
    let done: Vec<(u32, bool)> = st.active.iter().filter(|(&id, a)| over(id, a)).map(|(&id, a)| (id, a.fade_in)).collect();
    for (id, fade_in) in done {
        st.active.remove(&id);
        info!("fade: 0x{id:08X} {} done", if fade_in { "in" } else { "out" });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fade_goes_in_a_straight_line_over_its_length() {
        assert_eq!(fade_alpha(true, 0.0), 0.0);
        assert_eq!(fade_alpha(true, FADE_SECONDS / 2.0), 0.5);
        assert_eq!(fade_alpha(true, FADE_SECONDS * 2.0), 1.0);
        assert_eq!(fade_alpha(false, 0.0), 1.0);
        assert_eq!(fade_alpha(false, FADE_SECONDS / 4.0), 0.75);
        assert_eq!(fade_alpha(false, FADE_SECONDS), 0.0);
        assert_eq!(fade_alpha(true, -1.0), 0.0, "before its start: not yet drawn");
    }
}
