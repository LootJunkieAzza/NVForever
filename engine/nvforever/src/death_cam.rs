//! The death camera (owner request 2026-10-16): when the player dies the body drops as a
//! ragdoll, the view goes with it, the game slows down and the picture blurs.
//!
//! - **The body** (`ragdoll.rs`): the player's third-person rig is handed to its ragdoll
//!   ([`crate::ragdoll::PlayerRig`]) at the pose it died in, and its clips stop
//!   (`player_body.rs` leaves a dead player's body alone). It is drawn whole in both
//!   views, but for its head while the view is inside it.
//! - **First person**: the camera stays fixed to the head: where it was against the head
//!   bone when the player died, it stays, turning and falling with the head as the
//!   ragdoll drops.
//! - **Third person**: the camera keeps its place against the body's chest (it follows
//!   the body if it slides or rolls away) and turns to look at it over [`LOOK_IN`].
//! - Both views are kept in front of walls and floors between the bone they follow and
//!   the camera (the third-person camera's rule, `nvf_play::control::place_camera`), and
//!   eased towards where they want to be over [`FOLLOW`] so that the ragdoll's small
//!   shakes do not shake the picture.
//! - Without a body (`--no-actors`) or a ragdoll for its skeleton the view sinks to the
//!   floor where the player stood, turning over onto its side in first person.
//! - **Slow motion**: the game clock (`nvf_play::script::ClockScale`, on top of
//!   SetGlobalTimeMultiplier) eases down to [`SLOW`] over [`SLOW_IN`], stays there until
//!   [`SLOW_END`] and comes back to full speed over [`SLOW_OUT`] (seconds of real time,
//!   menus excluded): the fall plays out at a quarter speed. Sounds keep their speed (the
//!   virtual clock's rule).
//! - **Blur**: the picture blurs to radius [`BLUR`] (the image space modifiers' units,
//!   `imagespace.rs`) over [`BLUR_IN`] and stays blurred while the player is dead.
//!   `--death-imod <Editor ID or 0xFormID>` also plays that image space modifier when the
//!   player dies, as a script's ApplyImageSpaceModifier would.
//! - The player alive again (a save loaded, a script resurrecting): the ragdoll leaves
//!   the world, the body and the view are the player's again and the clock runs at full
//!   speed.
//!
//! Every constant here is [design]: how the original's death camera moves, how slow its
//! slow motion is and which image space modifier it plays, if any, are not known. The
//! original reloads the last save some seconds after the death; that is not built.
//!
//! Log lines: `death camera: the player died in first person: …` when it starts,
//! `death camera: the body is on the ground …` when the ragdoll comes to rest, and
//! `death camera: the player lives again …` when it ends.

use crate::limbs::ActorRig;
use crate::player_body::PlayerBody;
use crate::ragdoll::{Corpse, PlayerRig, Ragdolls};
use bevy::math::Affine3A;
use bevy::prelude::*;
use fnv_sim::FormId;
use nvf_assets::data::GameData;
use nvf_play::control::PlayerControl;
use nvf_play::phys::PhysState;
use nvf_play::player::Player;
use nvf_play::script::{ClockScale, ScriptState};
use nvf_play::{space, PlayerCamera};

/// The game clock's speed in slow motion, and the real seconds it takes to get there,
/// until it starts to come back, and to come back.
pub const SLOW: f32 = 0.25;
pub const SLOW_IN: f32 = 0.3;
pub const SLOW_END: f32 = 4.0;
pub const SLOW_OUT: f32 = 1.5;
/// The blur radius the picture goes to, and the real seconds it takes.
pub const BLUR: f32 = 4.0;
pub const BLUR_IN: f32 = 2.5;
/// Real seconds the third-person camera takes to turn to the body.
pub const LOOK_IN: f32 = 0.8;
/// The camera eases towards where it wants to be at this rate (per real second).
pub const FOLLOW: f32 = 12.0;
/// Without a body: the view sinks to this height above the feet over [`SINK`] seconds
/// of game time, and turns this far onto its side in first person.
pub const FLOOR_EYE: f32 = 15.0;
pub const SINK: f32 = 1.0;
pub const ROLL: f32 = 1.2;
/// The camera keeps this far in front of what it would go into, game units.
pub const WALL_MARGIN: f32 = 8.0;

/// The bones the third-person camera looks at, in order of preference.
const BODY_BONES: [&str; 3] = ["Bip01 Spine1", "Bip01 Spine", "Bip01 Pelvis"];
const HEAD_BONE: &str = "Bip01 Head";

/// The death camera's state.
#[derive(Resource, Default)]
pub struct DeathCam {
    /// The image space modifier played when the player dies (`--death-imod`).
    pub imod: Option<u32>,
    down: Option<Down>,
    /// Deaths shown, for the exit report.
    deaths: u32,
}

impl DeathCam {
    pub fn with_imod(imod: Option<u32>) -> Self {
        DeathCam { imod, ..default() }
    }

    /// The player is dead and the death camera has the view.
    pub fn down(&self) -> bool {
        self.down.is_some()
    }

    /// The player died in third person.
    pub fn third_person(&self) -> bool {
        self.down.as_ref().is_some_and(|d| d.third)
    }

    /// The blur radius on the picture now (`imagespace.rs`).
    pub fn blur(&self) -> f32 {
        self.down.as_ref().map_or(0.0, |d| blur_at(d.real))
    }
}

/// A dead player.
struct Down {
    third: bool,
    /// Real seconds since the death (menus excluded), and game seconds.
    real: f32,
    game: f32,
    /// The third-person rig's root and the rig, when its ragdoll took it.
    body: Option<(Entity, ActorRig)>,
    /// What the camera follows, from the first frame it is placed.
    view: Option<View>,
    /// The camera's last place.
    last: Option<Transform>,
    /// The ragdoll came to rest (told once).
    rested: bool,
}

/// Where the camera is held, against what.
enum View {
    /// First person: the camera's place and turn in the head bone's frame.
    Head { bone: usize, at: Vec3, turn: Quat },
    /// Third person: the camera's offset from the bone it looks at, and the turn it had.
    Body { bone: usize, offset: Vec3, from: Quat, since: f32 },
    /// No body: the camera as placed when the player died, and the feet (Bevy space).
    Sink { start: Transform, feet: Vec3 },
}

pub struct DeathCamPlugin;

impl Plugin for DeathCamPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DeathCam>()
            .add_systems(Update, watch.before(nvf_play::player::BodySet))
            .add_systems(Update, follow.after(crate::ragdoll::pose).run_if(resource_exists::<PhysState>))
            .add_systems(Last, report);
    }
}

fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// The game clock's speed `t` real seconds after the death.
pub fn slow_at(t: f32) -> f32 {
    if t < SLOW_IN {
        1.0 + (SLOW - 1.0) * smoothstep(t / SLOW_IN)
    } else if t < SLOW_END {
        SLOW
    } else {
        SLOW + (1.0 - SLOW) * smoothstep((t - SLOW_END) / SLOW_OUT)
    }
}

/// The blur radius `t` real seconds after the death.
pub fn blur_at(t: f32) -> f32 {
    BLUR * smoothstep(t / BLUR_IN)
}

/// Start the death camera when the player dies, end it when the player lives again, and
/// keep the clock's slow motion.
#[allow(clippy::too_many_arguments)]
fn watch(
    mut commands: Commands,
    (real, virt): (Res<Time<Real>>, Res<Time<Virtual>>),
    mut script: Option<ResMut<ScriptState>>,
    control: Res<PlayerControl>,
    body: Res<PlayerBody>,
    data: Option<Res<GameData>>,
    mut ragdolls: ResMut<Ragdolls>,
    mut phys: Option<ResMut<PhysState>>,
    mut cam: ResMut<DeathCam>,
    mut scale: Option<ResMut<ClockScale>>,
) {
    let player = FormId::PLAYER_REF.0;
    let dead = nvf_play::combat::is_dead(script.as_deref(), player);
    let cam = &mut *cam;
    match (cam.down.as_mut(), dead) {
        (None, true) => {
            let third = control.third_person_view();
            // A pose kept from an earlier death would put the body back at rest there.
            ragdolls.rest.remove(&player);
            let corpse = body.third_rig().and_then(|(root, rig)| Some((root, data.as_deref().and_then(|d| ragdolls.corpse(d, &rig))?, rig)));
            let body = corpse.map(|(root, c, rig)| {
                commands.entity(root).insert((PlayerRig(rig.clone()), c));
                (root, rig)
            });
            let played = match (cam.imod, script.as_deref_mut().and_then(|s| s.rt.as_mut())) {
                (Some(m), Some(rt)) => {
                    rt.game.apply_image_modifier(FormId(m));
                    format!(", image space modifier {m:#010x}")
                }
                _ => String::new(),
            };
            info!(
                "death camera: the player died in {} person: {}; slow motion ×{SLOW} for {SLOW_END:.1} s, blur {BLUR}{played}",
                if third { "third" } else { "first" },
                match (&body, third) {
                    (Some(_), false) => "the body drops as a ragdoll, the view stays on its head",
                    (Some(_), true) => "the body drops as a ragdoll, the camera stays on it",
                    (None, _) => "no ragdoll for the body, the view sinks to the floor",
                }
            );
            cam.deaths += 1;
            cam.down = Some(Down { third, real: 0.0, game: 0.0, body, view: None, last: None, rested: false });
        }
        (Some(d), true) => {
            if !virt.is_paused() {
                d.real += real.delta_secs();
                d.game += virt.delta_secs();
            }
            if let (false, Some(p)) = (d.rested, phys.as_deref()) {
                if d.body.is_some() && p.world.ragdoll_asleep(player) {
                    d.rested = true;
                    info!("death camera: the body is on the ground {:.1} s after the death ({:.1} s of game time)", d.real, d.game);
                }
            }
        }
        (Some(d), false) => {
            if let Some((root, _)) = d.body {
                commands.entity(root).remove::<(PlayerRig, Corpse)>();
            }
            if let Some(p) = phys.as_deref_mut() {
                p.world.remove_ragdoll(player);
            }
            ragdolls.rest.remove(&player);
            cam.down = None;
            info!("death camera: the player lives again: the body, the view and the clock are the player's");
        }
        (None, false) => {}
    }
    let want = cam.down.as_ref().map_or(1.0, |d| slow_at(d.real));
    if let Some(s) = scale.as_deref_mut().filter(|s| s.0 != want) {
        s.0 = want;
    }
}

/// The world transform of bone `i` of `rig`: its root's, then the local transforms down
/// to it (the ragdoll wrote them this frame; the root stands still once the player is
/// dead).
fn bone_world(rig: &ActorRig, i: usize, globals: &Query<&GlobalTransform>, transforms: &Query<&Transform, Without<PlayerCamera>>) -> Option<Affine3A> {
    let mut chain = Vec::new();
    let mut cur = Some(i);
    while let Some(n) = cur.filter(|&n| n > 0 && chain.len() < 256) {
        chain.push(n);
        cur = rig.skel.parents.get(n).copied().flatten();
    }
    let mut g = globals.get(*rig.nodes.first()?).ok()?.affine();
    for &n in chain.iter().rev() {
        g *= transforms.get(*rig.nodes.get(n)?).ok()?.compute_affine();
    }
    Some(g)
}

/// The place and turn (no scale) of a world transform.
fn place_of(a: &Affine3A) -> (Vec3, Quat) {
    let (_, r, t) = a.to_scale_rotation_translation();
    (t, r.normalize())
}

/// `to`, or short of it in front of what lies between `from` and it (Bevy space).
fn clear(phys: &mut PhysState, from: Vec3, to: Vec3) -> Vec3 {
    let d = to - from;
    let len = d.length();
    if len < 1e-3 {
        return to;
    }
    let filter = fnv_phys::Filter {
        exclude_layers: fnv_phys::layers::character_ignores().union(fnv_phys::layers::actor_volumes()),
        // The ragdoll's own bodies and loose clutter.
        no_dynamic: true,
        ..Default::default()
    };
    let (a, b) = (space::to_game(from), space::to_game(to));
    let dir = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    match phys.ray(a, dir, len + WALL_MARGIN, filter) {
        Some(h) => from + d / len * (h.distance - WALL_MARGIN).clamp(0.0, len),
        None => to,
    }
}

/// Hold the camera on the dead player's head or body.
#[allow(clippy::too_many_arguments)]
fn follow(
    real: Res<Time<Real>>,
    mut cam: ResMut<DeathCam>,
    player: Option<Res<Player>>,
    mut phys: ResMut<PhysState>,
    globals: Query<&GlobalTransform>,
    transforms: Query<&Transform, Without<PlayerCamera>>,
    mut cams: Query<&mut Transform, With<PlayerCamera>>,
) {
    let Some(d) = cam.down.as_mut() else { return };
    let Ok(mut t) = cams.single_mut() else { return };
    let bone = |names: &[&str]| d.body.as_ref().and_then(|(_, rig)| names.iter().find_map(|n| rig.skel.by_name.get(*n).copied()));
    let world = |i: usize| d.body.as_ref().and_then(|(_, rig)| bone_world(rig, i, &globals, &transforms)).map(|a| place_of(&a));
    if d.view.is_none() {
        // The camera as placed this frame, as the player died.
        let head = bone(&[HEAD_BONE]).filter(|_| !d.third);
        let chest = bone(&BODY_BONES).filter(|_| d.third);
        d.view = Some(match (head.and_then(|h| Some((h, world(h)?))), chest.and_then(|c| Some((c, world(c)?)))) {
            (Some((h, (at, turn))), _) => View::Head { bone: h, at: turn.inverse() * (t.translation - at), turn: turn.inverse() * t.rotation },
            (_, Some((c, (at, _)))) => View::Body { bone: c, offset: t.translation - at, from: t.rotation, since: d.real },
            _ => View::Sink { start: *t, feet: space::to_bevy(player.as_ref().map_or(space::to_game(t.translation), |p| p.feet)) },
        });
    }
    let want = match d.view.as_ref() {
        Some(View::Head { bone, at, turn }) => {
            let Some((head, r)) = world(*bone) else { return };
            Transform::from_translation(clear(&mut phys, head, head + r * *at)).with_rotation(r * *turn)
        }
        Some(View::Body { bone, offset, from, since }) => {
            let Some((chest, _)) = world(*bone) else { return };
            let at = clear(&mut phys, chest, chest + *offset);
            let look = Transform::from_translation(at).looking_at(chest, Vec3::Y).rotation;
            Transform::from_translation(at).with_rotation(from.slerp(look, smoothstep((d.real - since) / LOOK_IN)))
        }
        Some(View::Sink { start, feet }) => {
            let k = smoothstep(d.game / SINK);
            let floor = feet.y + FLOOR_EYE;
            let mut at = start.translation;
            if d.third {
                // Look down at where the player fell.
                let target = *feet + Vec3::Y * FLOOR_EYE;
                Transform::from_translation(at).with_rotation(start.rotation.slerp(Transform::from_translation(at).looking_at(target, Vec3::Y).rotation, k))
            } else {
                at.y += (floor.min(at.y) - at.y) * k;
                Transform::from_translation(at).with_rotation(start.rotation * Quat::from_rotation_z(ROLL * k))
            }
        }
        None => return,
    };
    let k = 1.0 - (-real.delta_secs() * FOLLOW).exp();
    let out = match d.last {
        Some(l) => Transform { translation: l.translation.lerp(want.translation, k), rotation: l.rotation.slerp(want.rotation, k).normalize(), scale: Vec3::ONE },
        None => want,
    };
    if out.translation.is_finite() && out.rotation.is_finite() {
        d.last = Some(out);
        *t = out;
    }
}

fn report(mut exit: MessageReader<AppExit>, cam: Res<DeathCam>) {
    if exit.read().last().is_none() || cam.deaths == 0 {
        return;
    }
    info!("death camera: {} death(s) shown{}", cam.deaths, if cam.down() { ", the player dead at exit" } else { "" });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_slows_holds_and_comes_back() {
        assert!((slow_at(0.0) - 1.0).abs() < 1e-6);
        assert!((slow_at(SLOW_IN) - SLOW).abs() < 1e-6);
        assert!((slow_at(SLOW_END - 0.01) - SLOW).abs() < 1e-6);
        assert!((slow_at(SLOW_END + SLOW_OUT) - 1.0).abs() < 1e-6);
        assert!((slow_at(100.0) - 1.0).abs() < 1e-6);
        // Never faster than the game's own speed nor slower than SLOW.
        for i in 0..200 {
            let s = slow_at(i as f32 * 0.05);
            assert!((SLOW - 1e-6..=1.0 + 1e-6).contains(&s), "{s}");
        }
    }

    #[test]
    fn the_blur_grows_and_stays() {
        assert_eq!(blur_at(0.0), 0.0);
        assert!(blur_at(BLUR_IN / 2.0) > 0.0 && blur_at(BLUR_IN / 2.0) < BLUR);
        assert!((blur_at(BLUR_IN) - BLUR).abs() < 1e-6);
        assert!((blur_at(60.0) - BLUR).abs() < 1e-6);
    }
}
