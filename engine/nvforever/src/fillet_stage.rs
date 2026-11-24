//! The five finger fillet table in 3D, as the mod stages it (docs/spec-notes/fillet.md §8,
//! §9): while a game is on at the table (nvf_play `minigames`, its play page) the player's
//! own third-person body stands at the table and plays the mod's clips, the mod's knife
//! in its right hand, and the camera looks down on the hand from behind it.
//!
//! - **Where**: the table's anchor is the reference after the knife's (the knife's form ID
//!   plus one) [observed: all five tables of "Knife Minigame.esp" pair the knife 0x…ADE, DC3,
//!   DC5, DC7, DC9 with the blood decal one form ID on, fillet.md §7], else 20 units below
//!   the knife [hypothesis: the anchors lie 20 below their knives]. The table's frame is
//!   the anchor turned to the way the player faced when the game began (§9). The body's
//!   feet go on the table's `LFFPoseOrigin` node, the camera on `LFFCameraTarget`, and the
//!   prompted letter is drawn where `LFFPrompt1..5` are on the screen (nodes of
//!   `meshes\Minigames\FFF\TableChalk.nif`, read from the file).
//! - **The body**: the base pose `Ready.kf` loops ([`crate::furniture::BodyOverride`]),
//!   `Enter.kf` plays over it first, then `Stab1..5.kf` or `Miss1..5.kf` at the gap for
//!   every press and `Win.kf` for a win, each once over the base (§8). The view is third
//!   person meanwhile; the player's own view comes back when the game ends.
//! - **The knife**: `meshes\Minigames\FFF\PlayerKnife.nif` under the body's
//!   `Bip01 R Hand`; the table's own knife (the activator) is hidden meanwhile (§8).
//! - **The camera**: from where it was to the target over 1 s with a smootherstep blend,
//!   pitching down to 60 degrees (§9) [engineering choice: the move starts at once, not
//!   after the mod's 0.15 s].
//!
//! Without the clips (the mod's meshes missing) the screen keeps its flat hand.

use crate::furniture::{BodyClip, BodyOverride};
use crate::player_body::PlayerBody;
use bevy::prelude::*;
use nvf_assets::actor::{ActorCtx, ActorParts};
use nvf_assets::anim_play::Clip;
use nvf_assets::model::{ModelCtx, PreparedModel};
use nvf_play::control::{PlayerControl, Pov};
use nvf_play::minigames::{FPage, FPose, FilletScreen, FilletStage};
use nvf_play::player::Player;
use nvf_play::sim::SimState;
use nvf_play::space::to_bevy;
use nvf_play::{PlayerCamera, RefVisual};
use fnv_sim::FormId;
use std::sync::Arc;

const DIR: &str = "meshes\\minigames\\fff\\";
/// The furniture serial the table's base pose plays under (any one furniture does not use).
const SERIAL: u32 = 0xF111_E700;
/// The camera's move to its place (seconds), and its pitch there (degrees down) (§9).
const CAMERA_MOVE: f32 = 1.0;
const CAMERA_PITCH: f32 = 60.0;
/// How high (units) the camera's way to the table arcs above the straight line.
const HEAD_ARC: f32 = 14.0;
/// The table's nodes as the mod's file places them, when it cannot be read [observed:
/// TableChalk.nif, fillet.md §8].
const POSE_ORIGIN: Vec3 = Vec3::new(0.0, 0.0, 0.15);
const CAMERA_TARGET: Vec3 = Vec3::new(0.0, -25.0, 44.0);
/// How far the body stands above the pose origin: with the mod's pose the left wrist rests
/// at `LFFRestWrist` exactly [observed: 2026-11-04, the wrist bone at (-3.0, -6.0, 2.8)],
/// and the glove and Pip-Boy of a dressed player then sink about 0.3-0.4 units into the
/// table on the little finger's side [observed: side view]. 0.5 lifts them clear and keeps
/// the wrist within the 0.5 (sum of axes) the mod itself allows it (§9) [engineering choice].
const LIFT: f32 = 0.5;
const PROMPTS: [Vec3; 5] = [Vec3::new(-8.773, 4.081, 0.2), Vec3::new(-6.462, 8.034, 0.2), Vec3::new(-3.565, 9.525, 0.2), Vec3::new(-0.223, 9.366, 0.2), Vec3::new(3.858, 4.769, 0.2)];

pub struct FilletStagePlugin;

impl Plugin for FilletStagePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Staged>()
            .add_systems(Update, stage.before(nvf_play::player::BodySet))
            .add_systems(Update, (attach_knife, camera).chain().after(nvf_play::control::place_camera));
    }
}

struct Clips {
    ready: Arc<Clip>,
    enter: Option<Arc<Clip>>,
    exit: Option<Arc<Clip>>,
    win: Option<Arc<Clip>>,
    stab: [Option<Arc<Clip>>; 5],
    miss: [Option<Arc<Clip>>; 5],
}

struct Table {
    knife: u32,
    /// Game space.
    anchor: Vec3,
    yaw: f32,
    pov: Pov,
    clips: Clips,
    pose_origin: Vec3,
    camera_target: Vec3,
    prompts: [Vec3; 5],
    knife_model: Option<Arc<PreparedModel>>,
    knife_entity: Option<Entity>,
    seen_pose: u32,
    over: Option<(Arc<Clip>, u32)>,
    serial: u32,
    /// Real seconds since the table was staged, and the camera's place then.
    since: f32,
    from: Option<Transform>,
    /// Leaving: when the exit clip ends (real seconds), and how long it runs; the camera
    /// goes back meanwhile.
    leaving: Option<(f32, f32)>,
}

impl Table {
    /// A point of the table's frame in game space.
    fn at(&self, local: Vec3) -> Vec3 {
        self.anchor + Quat::from_rotation_z(-self.yaw) * local
    }
}

#[derive(Resource, Default)]
struct Staged {
    table: Option<Table>,
    /// The knife whose clips did not load (told once).
    failed: Option<u32>,
}

/// The table's nodes in its own frame: pose origin, camera target, prompts.
fn table_nodes(models: &mut ModelCtx) -> (Vec3, Vec3, [Vec3; 5]) {
    let Some(pm) = models.model(&format!("{DIR}tablechalk.nif")) else { return (POSE_ORIGIN, CAMERA_TARGET, PROMPTS) };
    let world = |name: &str| -> Option<Vec3> {
        let mut i = pm.nodes.iter().position(|n| n.name.as_deref().is_some_and(|m| m.eq_ignore_ascii_case(name)))?;
        let mut a = pm.nodes[i].local;
        while let Some(p) = pm.nodes[i].parent {
            a = pm.nodes[p].local * a;
            i = p;
        }
        Some(Vec3::from(a.translation))
    };
    let prompts = std::array::from_fn(|k| world(&format!("LFFPrompt{}", k + 1)).unwrap_or(PROMPTS[k]));
    (world("LFFPoseOrigin").unwrap_or(POSE_ORIGIN), world("LFFCameraTarget").unwrap_or(CAMERA_TARGET), prompts)
}

#[allow(clippy::too_many_arguments)]
fn stage(
    mut staged: ResMut<Staged>,
    screen: Res<FilletScreen>,
    mut out: ResMut<FilletStage>,
    mut body: ResMut<BodyOverride>,
    (mut parts, mut models): (ResMut<ActorParts>, ModelCtx),
    (sim, player, mut control): (Option<Res<SimState>>, Option<Res<Player>>, ResMut<PlayerControl>),
    real: Res<Time<Real>>,
    mut commands: Commands,
    mut visuals: Query<(&RefVisual, &mut Visibility)>,
) {
    // The screen leaves the hand to this stage (no flat hand before it is on).
    out.present = true;
    out.flat_at = staged.failed;
    let f = screen.0.as_ref().filter(|f| f.page == FPage::Play && f.game.is_some());
    let now = real.elapsed_secs();
    // The game over at this table: the exit clip plays first (§8), then the body, the
    // view and the table's knife are as they were. Back at the table meanwhile: stay.
    if let Some(t) = staged.table.as_mut() {
        let here = f.is_some_and(|f| f.knife == t.knife);
        if here {
            t.leaving = None;
        } else if t.leaving.is_none() {
            match t.clips.exit.clone() {
                Some(c) => {
                    t.leaving = Some((now + c.length(), c.length()));
                    t.serial += 1;
                    t.over = Some((c, t.serial));
                }
                None => t.leaving = Some((now, 0.0)),
            }
        }
    }
    if staged.table.as_ref().is_some_and(|t| t.leaving.is_some_and(|(end, _)| now >= end)) {
        let t = staged.table.take().unwrap();
        if body.0.as_ref().is_some_and(|b| b.serial == SERIAL) {
            body.0 = None;
        }
        control.pov = t.pov;
        if let Some(e) = t.knife_entity {
            commands.entity(e).despawn();
        }
        for (r, mut v) in &mut visuals {
            if r.0 == t.knife {
                *v = Visibility::Inherited;
            }
        }
        out.on = false;
        out.prompts = [None; 5];
        info!("fillet stage: left the table {:#010X}", t.knife);
    }
    if let Some(t) = staged.table.as_mut().filter(|t| t.leaving.is_some()) {
        // The exit clip over the base pose, where the table was.
        let feet = t.at(t.pose_origin + Vec3::Z * LIFT);
        body.0 = Some(BodyClip { clip: t.clips.ready.clone(), looped: true, serial: SERIAL, pos: feet.to_array(), heading: t.yaw, lying: false, over: t.over.clone(), under: None });
        return;
    }
    let Some(f) = f else { return };
    if staged.table.is_none() && staged.failed != Some(f.knife) {
        let (Some(sim), Some(player)) = (sim.as_deref(), player.as_deref()) else { return };
        let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
        let mut clip = |name: &str| ctx.clip(&format!("{DIR}{name}.kf"));
        let Some(ready) = clip("ready") else {
            warn!("fillet stage: {DIR}ready.kf does not load: the table stays flat");
            staged.failed = Some(f.knife);
            return;
        };
        let clips = Clips {
            ready,
            enter: clip("enter"),
            exit: clip("exit"),
            win: clip("win"),
            stab: std::array::from_fn(|i| clip(&format!("stab{}", i + 1))),
            miss: std::array::from_fn(|i| clip(&format!("miss{}", i + 1))),
        };
        let pos = |id: u32| sim.refs.get(FormId(id)).map(|r| Vec3::from_array(r.pos));
        let Some(knife) = pos(f.knife) else { return };
        let anchor = pos(f.knife + 1).filter(|a| a.distance(knife) < 40.0).unwrap_or(knife - Vec3::Z * 20.0);
        let (pose_origin, camera_target, prompts) = table_nodes(&mut models);
        let knife_model = models.model(&format!("{DIR}playerknife.nif"));
        info!("fillet stage: at the table {:#010X}, anchor {anchor:.1}, facing {:.0} degrees", f.knife, player.heading.to_degrees());
        staged.table = Some(Table {
            knife: f.knife,
            anchor,
            yaw: player.heading,
            pov: control.pov,
            serial: 1,
            over: clips.enter.clone().map(|c| (c, 1)),
            clips,
            pose_origin,
            camera_target,
            prompts,
            knife_model,
            knife_entity: None,
            seen_pose: f.pose.0,
            since: real.elapsed_secs(),
            from: None,
            leaving: None,
        });
        for (r, mut v) in &mut visuals {
            if r.0 == f.knife {
                *v = Visibility::Hidden;
            }
        }
    }
    let Some(t) = staged.table.as_mut() else { return };
    if f.pose.0 != t.seen_pose {
        t.seen_pose = f.pose.0;
        let clip = match f.pose.1 {
            FPose::Stab(g) => t.clips.stab.get(g as usize - 1).cloned().flatten(),
            FPose::Miss(g) => t.clips.miss.get(g as usize - 1).cloned().flatten(),
            FPose::Win => t.clips.win.clone(),
            FPose::Ready => None,
        };
        if let Some(c) = clip {
            t.serial += 1;
            t.over = Some((c, t.serial));
        }
    }
    control.pov = Pov::Third;
    // Only the mod's knife is in the hand: a weapon out goes away (the body hides one
    // still in the hand while it plays the table's clips, `player_body.rs`).
    control.weapon_out = false;
    let feet = t.at(t.pose_origin + Vec3::Z * LIFT);
    body.0 = Some(BodyClip { clip: t.clips.ready.clone(), looped: true, serial: SERIAL, pos: feet.to_array(), heading: t.yaw, lying: false, over: t.over.clone(), under: None });
    out.on = true;
}

/// The mod's knife into the body's right hand, once the body is there.
fn attach_knife(mut staged: ResMut<Staged>, bodies: Res<PlayerBody>, mut commands: Commands) {
    let Some(t) = staged.table.as_mut() else { return };
    if t.knife_entity.is_some() {
        return;
    }
    let Some(pm) = t.knife_model.clone() else { return };
    let animators = bodies.animators();
    let Some((_, a)) = animators.first() else { return };
    let Some(&i) = a.skel.by_name.get("Bip01 R Hand") else { return };
    let Some(&hand) = a.nodes.get(i) else { return };
    let holder = commands.spawn((Transform::IDENTITY, Visibility::Inherited, ChildOf(hand))).id();
    let nodes: Vec<Entity> = pm.nodes.iter().map(|n| commands.spawn((Transform::from_matrix(Mat4::from(n.local)), Visibility::Inherited)).id()).collect();
    for (n, e) in pm.nodes.iter().zip(&nodes) {
        commands.entity(n.parent.map_or(holder, |p| nodes[p])).add_child(*e);
    }
    for s in &pm.shapes {
        let e = commands.spawn((Mesh3d(s.mesh.clone()), MeshMaterial3d(s.material.clone()), Transform::from_matrix(Mat4::from(s.own)), Visibility::Inherited)).id();
        commands.entity(s.node.map_or(holder, |n| nodes[n])).add_child(e);
    }
    t.knife_entity = Some(holder);
}

fn smootherstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * t * (t * (6.0 * t - 15.0) + 10.0)
}

/// The camera behind and above the hand, looking down, and the prompts on the screen.
fn camera(mut staged: ResMut<Staged>, real: Res<Time<Real>>, mut out: ResMut<FilletStage>, mut cams: Query<(&mut Transform, &Camera), With<PlayerCamera>>) {
    let Some(t) = staged.table.as_mut() else { return };
    let Ok((mut cam, camera)) = cams.single_mut() else { return };
    let from = *t.from.get_or_insert(*cam);
    let at = to_bevy(t.at(t.camera_target).to_array());
    let pitch = CAMERA_PITCH.to_radians();
    let ahead = Vec3::new(t.yaw.sin() * pitch.cos(), t.yaw.cos() * pitch.cos(), -pitch.sin());
    let target = Transform::from_translation(at).looking_at(to_bevy((t.at(t.camera_target) + ahead).to_array()), Vec3::Y);
    // Going: back from the table to where the camera was, over the exit clip.
    let k = match t.leaving {
        Some((end, len)) if len > 0.0 => smootherstep((end - real.elapsed_secs()) / len),
        Some(_) => 0.0,
        None => smootherstep((real.elapsed_secs() - t.since) / CAMERA_MOVE),
    };
    // Over the posed head, not through it (the straight line from the player's eye passes
    // inside the head and the view goes black) [engineering choice].
    let over_head = Vec3::Y * HEAD_ARC * (k * std::f32::consts::PI).sin();
    *cam = Transform { translation: from.translation.lerp(target.translation, k) + over_head, rotation: from.rotation.slerp(target.rotation, k), scale: Vec3::ONE };
    let size = camera.logical_viewport_size();
    let global = GlobalTransform::from(*cam);
    out.prompts = std::array::from_fn(|i| {
        let p = camera.world_to_viewport(&global, to_bevy(t.at(t.prompts[i]).to_array())).ok()?;
        let s = size?;
        Some((p.x / s.x, p.y / s.y))
    });
}
