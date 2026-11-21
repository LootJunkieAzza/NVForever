//! The combat face (docs/spec-notes/combat-face.md): an actor in the Combat alert state
//! (`fnv_game::awareness`, alert.rs) shows the `CombatAnger` mood morph of its head
//! parts. No combat animation carries it [observed: no `.kf` names it], so it is set
//! here from the state.
//!
//! - **Who**: drawn actors with a face rig (lips.rs `LipRig`: humans and ghouls; creatures
//!   and robots have no `.tri`). The player is not driven.
//! - **When**: the weight goes to 1 while the actor is in Combat and alive, to 0 otherwise
//!   (Alert, Lost, Normal, dead), linearly over [`FADE_SECONDS`] each way, in game time
//!   (it holds under pause) [hypothesis: "a second or two to initiate", GECK wiki
//!   ModifyFaceGen; strength and the way out are not known].
//! - **How**: it sets the `CombatAnger` weight of the actor's [`FaceMorphs`]; lips.rs draws
//!   it, adding the lip sync's mouth shapes on top, so a fighter's barks still move its
//!   mouth.
//!
//! Log lines (named as the alert lines name actors): `face: <name> <ref> mood CombatAnger rises|falls` when the target changes,
//! and `face: <name> <ref> mood CombatAnger <weight>` when the fade has arrived.

use crate::face_morphs::FaceMorphs;
use crate::limbs::ActorRig;
use crate::lips::LipRig;
use bevy::prelude::*;
use fnv_sim::FormId;
use nvf_play::script::ScriptState;

/// The `.tri` label of the combat face.
pub const LABEL: &str = "CombatAnger";

/// Seconds from no anger to full anger, and back [hypothesis].
pub const FADE_SECONDS: f32 = 1.0;

/// The weight at full anger [hypothesis: the whole morph, MFG 100].
pub const FULL: f32 = 1.0;

/// `weight` moved towards `target` by `dt` seconds of the fade.
pub fn step(weight: f32, target: f32, dt: f32) -> f32 {
    let reach = dt.max(0.0) * FULL / FADE_SECONDS;
    if weight < target {
        (weight + reach).min(target)
    } else {
        (weight - reach).max(target)
    }
}

/// On an actor's root while this module shows anger on it: the weight it set and the
/// weight it fades to. An actor without it is left alone, so a CombatAnger weight set by
/// something else (a test aid) is not taken over.
#[derive(Component)]
struct CombatFace {
    weight: f32,
    target: f32,
}

type Actors<'w> = (Entity, &'w ActorRig, Option<&'w Name>, Option<&'w mut FaceMorphs>, Option<&'w mut CombatFace>);

fn drive(mut commands: Commands, time: Res<Time>, script: Option<Res<ScriptState>>, mut actors: Query<Actors, With<LipRig>>) {
    let Some(game) = script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game) else { return };
    let dt = time.delta_secs();
    for (e, rig, name, morphs, own) in &mut actors {
        let id = FormId(rig.reference);
        let fighting = game.awareness.state(id).in_combat() && !game.is_dead(id);
        if own.is_none() && !fighting {
            continue;
        }
        let target = if fighting { FULL } else { 0.0 };
        // As the alert lines name it (detection.rs): "Actor 000F570B 1EFiendViolet" → "1EFiendViolet 000F570B".
        let who = || format!("{} {:08X}", name.and_then(|n| n.as_str().split_whitespace().nth(2)).unwrap_or(""), rig.reference).trim().to_string();
        let (weight, was) = own.as_ref().map_or((0.0, None), |o| (o.weight, Some(o.target)));
        if was != Some(target) {
            info!("face: {} mood {LABEL} {}", who(), if target > weight { "rises" } else { "falls" });
        }
        let next = step(weight, target, dt);
        match own {
            Some(mut o) => (o.weight, o.target) = (next, target),
            None => {
                commands.entity(e).try_insert(CombatFace { weight: next, target });
            }
        }
        match morphs {
            Some(mut m) => {
                if m.get(LABEL) != next {
                    m.set(LABEL, next);
                }
            }
            None => {
                let mut m = FaceMorphs::default();
                m.set(LABEL, next);
                commands.entity(e).try_insert(m);
            }
        }
        if next == target && weight != target {
            info!("face: {} mood {LABEL} {next:.2}", who());
        }
        if next == 0.0 && target == 0.0 {
            commands.entity(e).remove::<CombatFace>();
        }
    }
}

pub struct CombatFacePlugin;

impl Plugin for CombatFacePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, drive.after(nvf_play::PlaySet::Scripts));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_face_fades_in_and_out_over_the_fade_time() {
        let half = FADE_SECONDS / 2.0;
        assert!((step(0.0, FULL, half) - FULL / 2.0).abs() < 1e-6);
        assert_eq!(step(0.9, FULL, half), FULL);
        assert!((step(FULL, 0.0, half) - FULL / 2.0).abs() < 1e-6);
        assert_eq!(step(0.1, 0.0, half), 0.0);
        assert_eq!(step(0.4, 0.4, 1.0), 0.4);
        assert_eq!(step(0.4, FULL, -1.0), 0.4);
    }
}
