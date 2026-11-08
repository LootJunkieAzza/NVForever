//! Test aid `NVF_PLAY_TEST_POSE_WATCH=1`: checks on what is drawn, a diagnostic only
//! (it changes nothing).
//!
//! - **Pose**: a living character whose body is far off upright (owner's overnight queue
//!   item 85, 2026-11-07: "goodsprings settler raking animation occasionally lies
//!   sideways on the ground for a frame"). Each frame, after the bones are placed, the
//!   pelvis-to-head line of every drawn character (not creatures, whose spines may lie
//!   level) is measured against the vertical; one that is not dead, dying, knocked out
//!   or on furniture (where lying is meant) and leans more than [`OFF_UPRIGHT_DEGREES`]
//!   gets a log line `pose watch: …` with the idles it shows, their clip times and
//!   weights (`idles::ActorIdle`), or "no idle".
//! - **Securitrons** ([`RobotWatch`], item 89): one drawn near and ahead of the camera
//!   that stops being drawn gets a line `robot watch: …` saying how.
//!
//! Summaries at exit: `pose watch summary: …`, `robot watch summary: …`.

use crate::limbs::ActorRig;
use crate::locomotion::Dying;
use crate::ragdoll::Corpse;
use crate::furniture::OnFurniture;
use crate::idles::ActorIdle;
use crate::knockouts::{bone, KnockedOut};
use bevy::app::AppExit;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use nvf_play::RefVisual;

/// A body leaning more than this off the vertical, pelvis to head, is off upright
/// [our choice: a standing idle's lean (raking, sweeping, leaning on a rail) stays well
/// under 45°; one frame of the skeleton's raw pose reads near 90°].
pub const OFF_UPRIGHT_DEGREES: f32 = 60.0;

pub struct PoseWatchPlugin;

impl Plugin for PoseWatchPlugin {
    fn build(&self, app: &mut App) {
        // A test aid: on only when asked for (`NVF_PLAY_TEST_POSE_WATCH=1`).
        if std::env::var("NVF_PLAY_TEST_POSE_WATCH").as_deref() != Ok("1") {
            return;
        }
        app.init_resource::<PoseWatch>()
            .init_resource::<RobotWatch>()
            .add_systems(PostUpdate, (watch.after(TransformSystems::Propagate), watch_robots.after(bevy::camera::visibility::VisibilitySystems::CheckVisibility)))
            .add_systems(Last, report);
    }
}

#[derive(Resource, Default)]
struct PoseWatch {
    /// Frames an actor showing an idle was off upright, and frames checked.
    off: u64,
    checked: u64,
    /// Lines logged (at most [`MAX_LINES`]).
    lines: u32,
}

const MAX_LINES: u32 = 40;

/// A skeleton that stands upright: the characters' (`characters\_male\skeleton.nif` and
/// its kin); a creature's spine may lie level.
fn is_human(rig: &ActorRig) -> bool {
    rig.skeleton.to_ascii_lowercase().replace('/', "\\").contains(r"characters\_male")
}

type Watched = (&'static RefVisual, &'static ActorRig, Option<&'static ActorIdle>, Has<Dying>, Has<Corpse>, Has<KnockedOut>, Has<OnFurniture>);

fn watch(mut w: ResMut<PoseWatch>, actors: Query<Watched>, globals: Query<&GlobalTransform>, time: Res<Time>) {
    for (r, rig, idle, dying, corpse, knocked, furniture) in &actors {
        // Frames with no idle showing are watched too: the frame between two idles is the
        // one most likely to show the skeleton's raw pose.
        if dying || corpse || knocked || furniture || !is_human(rig) {
            continue;
        }
        let (Some(p), Some(h)) = (bone(rig, &globals, "Bip01 Pelvis"), bone(rig, &globals, "Bip01 Head")) else { continue };
        let (p, h) = (Vec3::from(nvf_play::space::to_game(p)), Vec3::from(nvf_play::space::to_game(h)));
        let up = h - p;
        if up.length() < 1.0 {
            continue;
        }
        w.checked += 1;
        let off = up.normalize().z.clamp(-1.0, 1.0).acos().to_degrees();
        if off <= OFF_UPRIGHT_DEGREES {
            continue;
        }
        w.off += 1;
        if w.lines < MAX_LINES {
            w.lines += 1;
            let shown: Vec<String> = idle
                .into_iter()
                .flat_map(|i| i.0.shown())
                .map(|(pl, wt)| format!("{:08X} {} at {:.3}/{:.3} s weight {wt:.2}{}", pl.idle, pl.clip.clip.name, pl.play.time(), pl.play.keys.length, if pl.play.ended() { " (ended)" } else { "" }))
                .collect();
            info!("pose watch: {:08X} is {off:.0} degrees off upright at {:.3} s, showing {}", r.0, time.elapsed_secs(), if shown.is_empty() { "no idle".to_string() } else { shown.join("; ") });
        }
    }
}

/// Securitrons drawn last frame, by reference: where they were, and whether they were
/// near and ahead of the camera (owner's overnight queue item 89, 2026-11-07:
/// "occasionally securitrons are disappearing from the strip"). One that was drawn there
/// and is not drawn now gets a log line saying how it went: its entity gone, hidden
/// (`InheritedVisibility`), or every mesh of it culled (`ViewVisibility`).
#[derive(Resource, Default)]
struct RobotWatch {
    seen: std::collections::HashMap<u32, (Vec3, bool)>,
    lost: u32,
    lines: u32,
}

/// Near and ahead: within this many units of the camera and this many degrees of its
/// forward direction [our choice: well inside the view].
const ROBOT_NEAR: f32 = 3000.0;
const ROBOT_AHEAD_DEGREES: f32 = 30.0;

fn is_securitron(rig: &ActorRig) -> bool {
    rig.skeleton.to_ascii_lowercase().contains("nvsecuritron")
}

#[allow(clippy::type_complexity)]
fn watch_robots(
    mut w: ResMut<RobotWatch>,
    robots: Query<(Entity, &RefVisual, &ActorRig, &InheritedVisibility, &GlobalTransform, Has<Dying>)>,
    children: Query<&Children>,
    views: Query<&ViewVisibility, With<Mesh3d>>,
    camera: Query<&GlobalTransform, With<nvf_play::PlayerCamera>>,
    time: Res<Time>,
) {
    let Ok(cam) = camera.single() else { return };
    let (eye, fwd) = (cam.translation(), cam.forward().as_vec3());
    let ahead = |p: Vec3| {
        let d = p - eye;
        d.length() < ROBOT_NEAR && d.normalize_or_zero().dot(fwd) > ROBOT_AHEAD_DEGREES.to_radians().cos()
    };
    let mut now: std::collections::HashMap<u32, (Vec3, bool)> = std::collections::HashMap::new();
    let mut why: std::collections::HashMap<u32, &'static str> = std::collections::HashMap::new();
    for (e, r, rig, inherited, gt, dying) in &robots {
        if !is_securitron(rig) {
            continue;
        }
        let p = gt.translation() + Vec3::Y * 80.0;
        let drawn = inherited.get() && children.iter_descendants(e).any(|c| views.get(c).is_ok_and(|v| v.get()));
        now.insert(r.0, (p, drawn));
        if !drawn {
            why.insert(r.0, if !inherited.get() { "hidden" } else if dying { "every mesh culled (dead)" } else { "every mesh culled" });
        }
    }
    for (&id, &(p, was)) in &w.seen.clone() {
        if !was || !ahead(p) {
            continue;
        }
        let gone = match now.get(&id) {
            None => "its entity is gone",
            Some((_, true)) => continue,
            Some(_) => why.get(&id).copied().unwrap_or("not drawn"),
        };
        w.lost += 1;
        if w.lines < MAX_LINES {
            w.lines += 1;
            let at = nvf_play::space::to_game(p);
            info!("robot watch: Securitron {id:08X} stopped being drawn at {:.3} s, {:.0} units ahead of the camera at ({:.0}, {:.0}, {:.0}): {gone}", time.elapsed_secs(), (p - eye).length(), at[0], at[1], at[2]);
        }
    }
    w.seen = now;
}

fn report(mut exit: MessageReader<AppExit>, w: Res<PoseWatch>, robots: Res<RobotWatch>) {
    if exit.read().next().is_some() {
        info!("robot watch summary: {} times a Securitron near and ahead of the camera stopped being drawn", robots.lost);
        info!("pose watch summary: {} of {} frames of living actors were more than {OFF_UPRIGHT_DEGREES:.0} degrees off upright", w.off, w.checked);
    }
}
