//! Emotes played (the fork's; queue item 137; the catalogue, the wheel and the loadout are
//! `nvf_play::emotes`).
//!
//! **This player**: an emote plays on the third-person body as furniture's clips do
//! (`crate::furniture::BodyOverride`), over the standing idle (`STAND`) for the bones its
//! clip does not key. The body holds `Bip01` still and is drawn where the emote started
//! for the whole of it: the clips' root tracks move nothing (followed, the entry and exit
//! clips walked the player forward a little each time, into a tree by the end of a
//! laptop run, 2026-11-09). The character is held there (`PlayerSeat::hold`: it collides
//! with nothing, so it pushes no one) and set on the ground under it again when the emote
//! ends. The view goes to third person for the emote and back after it [design: the
//! owner's choice, 2026-11-09]; the mouse then circles the camera round the body, which
//! keeps its heading. The weapon is put away. A loop ends (its exit clip, the fast one
//! when a fight broke in) on a movement key or jump, Emote tapped or its wheel opened, the
//! weapon drawn, a hit taken or combat; a one-shot ends at once on the same. No emote
//! starts in combat, swimming, in the air, on furniture or outside play (a conversation,
//! a menu).
//!
//! **The others**: what this player plays goes out in its state (`PlayerState::emote`:
//! the emote, its clip, phase and time, `coop.rs`), its yaw the emote's heading; each
//! puppet plays what its player sends as a full-body idle, from where its player is in
//! it when first seen (one who joins late sees it under way). Its name tag follows its head
//! bone, in an emote or not (`crate::mate_heads`).
//!
//! **On the ground**: every emote clip, the player's and the puppets', is first set on the
//! ground ([`EmoteGround`]): its lowest bone at its first frame level with the standing
//! idle's. The game's bed idles lie at a mattress's height; set down, lay-back lies on the
//! ground. The start is on the ground under the feet by a trace down.
//!
//! Log lines: `emotes: …` (start, each phase, end, refusals; the puppets' as
//! `emotes: puppet …`; `emotes: ground: …` per clip as first played).
//!
//! Test aid: `NVF_PLAY_TEST_EMOTE_GROUND=1` sets every catalogue clip the load order has on
//! the ground at the start of play (one `emotes: ground:` line each), for an audit.

use crate::furniture::{BodyClip, BodyOverride};
use crate::idles::ActorIdle;
use crate::limbs::ActorRig;
use bevy::prelude::*;
use fnv_actor::furniture::Frame;
use fnv_actor::idle_play::{IdleKeys, IdlePlay};
use fnv_game::idles::IdleSource;
use fnv_nif::anim::Channel;
use nvf_assets::actor::{bind_clip, ActorCtx, ActorParts, PreparedSkeleton};
use nvf_assets::anim_blend::{Animator, Mode};
use nvf_assets::anim_play::Clip;
use nvf_assets::model::ModelCtx;
use nvf_coop::net::{emote as phase, EmoteState};
use nvf_play::bindings::{Control, ControlInput};
use nvf_play::control::Pov;
use nvf_play::emotes::{vanilla, EmoteAsk, EmoteBook, Shape, CATALOG};
use nvf_play::PlayState;
use std::collections::HashMap;
use std::sync::Arc;

/// The standing idle under an emote's clip [observed: `Fallout - Meshes.bsa`].
const STAND: &str = "meshes\\characters\\_male\\locomotion\\mtidle.kf";

/// What this player's emote sends (`coop.rs`): the state, and the heading the body keeps.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct LocalEmote {
    pub state: EmoteState,
    pub heading: Option<f32>,
}

/// What the other players emote, by their puppets' references (written by `coop.rs`).
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct PeerEmotes(pub Vec<(u32, EmoteState)>);

/// An emote under way on this player.
struct Run {
    emote: usize,
    variant: u8,
    phase: u8,
    step: u8,
    /// Seconds into the clip.
    t: f32,
    clip: Arc<Clip>,
    path: String,
    /// Where the emote plays: the feet and heading at its start.
    frame: Frame,
    /// The standing idle under the clip.
    stand: Option<Arc<Clip>>,
    /// Seconds since it started, and when a test aid ends it.
    age: f32,
    cancel_at: Option<f32>,
}

#[derive(Resource, Default)]
pub struct EmotePlay {
    run: Option<Run>,
    serial: u32,
    /// The view to go back to when the emote ends (first person, put to third for it).
    restore: Option<Pov>,
    /// The player's hits counted so far (`CombatStats::player_hits`).
    hits: u32,
    seed: u64,
    started: u32,
    refused: u32,
    puppets: HashMap<u32, Puppet>,
}

/// The skeleton emotes are measured on [observed: `Fallout - Meshes.bsa`].
const SKELETON: &str = "meshes\\characters\\_male\\skeleton.nif";
/// The node a clip's height is moved on.
const NON_ACCUM: &str = "Bip01 NonAccum";
/// A clip whose lowest bone is within this of the standing idle's is left as it is (game
/// units) [design].
const SNAP_MIN: f32 = 3.0;

/// The emote clips set on the ground, by path: each one's lowest bone at its first frame
/// (over the standing idle, as the body plays it) is put level with the standing idle's
/// lowest bone, by moving its `Bip01 NonAccum` track up or down. The game's bed idles
/// (lay-back's `specialidle_nvsleeponback.kf`) lie at a mattress's height and floated in
/// the air (laptop run 139, 2026-11-09). Measured from the data alike on every machine,
/// so the player and every viewer's puppet play the same clip and nothing more goes
/// over the wire.
#[derive(Resource, Default)]
pub struct EmoteGround {
    /// The standing idle's lowest bone height, once measured.
    stand: Option<Option<f32>>,
    clips: HashMap<String, Option<Arc<Clip>>>,
    /// The clips moved, for the summary.
    moved: Vec<(String, f32)>,
}

impl EmoteGround {
    /// The clip at `path`, set on the ground.
    pub fn clip(&mut self, ctx: &mut ActorCtx, path: &str) -> Option<Arc<Clip>> {
        if let Some(c) = self.clips.get(path) {
            return c.clone();
        }
        let out = ctx.clip(path).map(|raw| self.ground(ctx, path, raw));
        self.clips.insert(path.to_string(), out.clone());
        out
    }

    fn ground(&mut self, ctx: &mut ActorCtx, path: &str, raw: Arc<Clip>) -> Arc<Clip> {
        let raw = sane(path, raw);
        let (Some(skel), Some(stand)) = (ctx.skeleton(SKELETON), ctx.clip(STAND)) else { return raw };
        let reference = *self.stand.get_or_insert_with(|| lowest(&skel, &[&stand]).map(|(z, _)| z));
        let (Some(reference), Some((low, bone))) = (reference, lowest(&skel, &[&stand, &raw])) else { return raw };
        let lift = reference - low;
        if lift.abs() < SNAP_MIN {
            info!("emotes: ground: {path}: lowest bone {bone} at {low:.1} ({lift:+.1} from the standing idle's): left as it is");
            return raw;
        }
        match raised(&raw, NON_ACCUM, lift) {
            Some(c) => {
                info!("emotes: ground: {path}: lowest bone {bone} at {low:.1}: moved {lift:+.1} to the standing idle's {reference:.1}");
                self.moved.push((path.to_string(), lift));
                Arc::new(c)
            }
            None => {
                info!("emotes: ground: {path}: lowest bone {bone} at {low:.1} ({lift:+.1}), no {NON_ACCUM} translation to move: left as it is");
                raw
            }
        }
    }
}

/// `NVF_PLAY_TEST_EMOTE_GROUND`: every catalogue clip set on the ground, once.
fn ground_audit(mut done: Local<bool>, data: Option<Res<nvf_assets::GameData>>, mut ground: ResMut<EmoteGround>, (mut parts, mut models): (ResMut<ActorParts>, ModelCtx)) {
    if *done || data.is_none() {
        return;
    }
    *done = true;
    let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
    let mut n = 0;
    for (i, e) in CATALOG.iter().enumerate() {
        let fast = clip_path(i, 1, phase::EXIT, 0);
        for p in e.candidates().into_iter().chain(e.needs()).chain(fast) {
            if !ground.clips.contains_key(&p) && ground.clip(&mut ctx, &p).is_some() {
                n += 1;
            }
        }
    }
    let moved: Vec<String> = ground.moved.iter().map(|(p, dz)| format!("{} {dz:+.1}", p.rsplit('\\').next().unwrap_or(p))).collect();
    info!("test aid NVF_PLAY_TEST_EMOTE_GROUND: {n} clips measured, {} moved: {}", moved.len(), moved.join(", "));
}

/// The lowest `Bip01 …` bone of the body (its height and name) of `clips` at their first frames, each
/// over the ones before it as the body plays an emote over the standing idle.
fn lowest(skel: &Arc<PreparedSkeleton>, clips: &[&Arc<Clip>]) -> Option<(f32, String)> {
    let mut a = Animator::new(skel.clone(), Vec::new(), Some("Bip01"));
    for (k, c) in clips.iter().enumerate() {
        let i = a.want(c, k as u32, 1.0, Mode::Once, 0.0);
        if k > 0 {
            a.lift(i, 1000 * k as i32);
        }
    }
    a.pose();
    skel.names
        .iter()
        .enumerate()
        .skip(1)
        .filter_map(|(i, n)| {
            let n = n.as_deref()?;
            // The pauldrons are armour's shoulder pads, not the body: lying, the floor sleep
            // has one under the ground and was moved up 3 in its loop and exit alone.
            (n.starts_with("Bip01 ") && n != NON_ACCUM && !n.contains("Pauldron")).then(|| (a.model_space(i).translation.z, n.to_string()))
        })
        .min_by(|x, y| x.0.total_cmp(&y.0))
}

/// Bounds an emote clip's tracks are kept within, so one bad key cannot fill a viewer's
/// screen with a puppet: a bone's scale (a body's bones keep theirs at 1) and the root's
/// distance from the actor (game units; `Bip01 NonAccum` stands at the pelvis, 66 up)
/// [design].
const SCALE_BOUNDS: (f32, f32) = (0.5, 2.0);
const ROOT_REACH: f32 = 150.0;

/// `clip` with what is out of bounds left out (logged): a track's scale past
/// [`SCALE_BOUNDS`] at any key, the root's or `Bip01 NonAccum`'s translation past
/// [`ROOT_REACH`]. The bone then takes its bind pose's part, as for a clip that does not
/// key it.
fn sane(path: &str, clip: Arc<Clip>) -> Arc<Clip> {
    let values = |c: &Channel<1>| -> Vec<f32> {
        match c {
            Channel::Static(Some([s])) => vec![*s],
            Channel::Keys(k) => k.keys.iter().map(|k| k.value[0]).collect(),
            Channel::BSpline { spline, .. } => spline.points.clone(),
            _ => Vec::new(),
        }
    };
    let reach = |c: &Channel<3>| -> f32 {
        let len = |v: &[f32]| v.iter().map(|x| x * x).sum::<f32>().sqrt();
        match c {
            Channel::Static(Some(v)) => len(v),
            Channel::Keys(k) => k.keys.iter().map(|k| len(&k.value)).fold(0.0, f32::max),
            Channel::BSpline { spline, .. } if spline.dim == 3 => spline.points.chunks(3).map(len).fold(0.0, f32::max),
            _ => 0.0,
        }
    };
    let root = clip.accum_root.clone().unwrap_or_else(|| "Bip01".to_string());
    let mut out: Option<Clip> = None;
    for i in 0..clip.tracks.len() {
        let (name, track) = &clip.tracks[i];
        let bad_scale = values(&track.scale).into_iter().find(|s| !(SCALE_BOUNDS.0..=SCALE_BOUNDS.1).contains(s));
        let far = (*name == root || name == NON_ACCUM).then(|| reach(&track.translation)).filter(|r| *r > ROOT_REACH);
        if bad_scale.is_none() && far.is_none() {
            continue;
        }
        let mut t = (**track).clone();
        if let Some(s) = bad_scale {
            warn!("emotes: ground: {path}: {name} scale {s} is out of bounds: left out");
            t.scale = Channel::Static(None);
        }
        if let Some(r) = far {
            warn!("emotes: ground: {path}: {name} reaches {r:.0} units from the actor: its translation left out");
            t.translation = Channel::Static(None);
        }
        out.get_or_insert_with(|| (*clip).clone()).tracks[i].1 = Arc::new(t);
    }
    out.map_or(clip, Arc::new)
}

/// `clip` with node `node`'s translation moved up by `dz`; `None` when the clip has no
/// translation of its own for it.
fn raised(clip: &Clip, node: &str, dz: f32) -> Option<Clip> {
    let i = clip.tracks.iter().position(|(n, _)| n == node)?;
    let mut t = (*clip.tracks[i].1).clone();
    match &mut t.translation {
        Channel::Static(Some(v)) => v[2] += dz,
        Channel::Keys(k) if !k.keys.is_empty() => k.keys.iter_mut().for_each(|key| key.value[2] += dz),
        Channel::BSpline { spline, .. } if spline.dim == 3 => spline.points.chunks_mut(3).for_each(|p| p[2] += dz),
        _ => return None,
    }
    let mut c = clip.clone();
    c.tracks[i].1 = Arc::new(t);
    Some(c)
}

/// What a puppet plays.
struct Puppet {
    key: (u8, u8, u8, u8),
    serial: u32,
}

/// A clip of emote `emote`: a one-shot's candidate `variant`; a loop's entry, `step`-th
/// staying clip, or exit (`variant` 1: the fast one, where it has one).
pub fn clip_path(emote: usize, variant: u8, ph: u8, step: u8) -> Option<String> {
    let e = CATALOG.get(emote)?;
    match (e.shape, ph) {
        (Shape::Once { .. }, _) => e.candidates().get(variant as usize).cloned(),
        (Shape::Loop { enter, .. }, phase::ENTER) => enter.map(vanilla),
        (Shape::Loop { stay, .. }, phase::STAY) => (!stay.is_empty()).then(|| vanilla(stay[step as usize % stay.len()])),
        (Shape::Loop { exit, fast, .. }, phase::EXIT) => (if variant == 1 { fast.or(exit) } else { exit }).map(vanilla),
        _ => None,
    }
}

/// A staying clip that loops on its own (the loop has one).
fn loops_alone(emote: usize, ph: u8) -> bool {
    ph == phase::STAY && matches!(CATALOG.get(emote).map(|e| e.shape), Some(Shape::Loop { stay, .. }) if stay.len() == 1)
}

fn phase_name(ph: u8) -> &'static str {
    match ph {
        phase::ENTER => "enter",
        phase::STAY => "loop",
        phase::EXIT => "exit",
        _ => "once",
    }
}

/// Why this player cannot start an emote now, in our own words.
fn refusal(playing: bool, combat: bool, swimming: bool, airborne: bool, furniture: bool, installed: bool) -> Option<&'static str> {
    if !installed {
        Some("That emote is not installed.")
    } else if !playing {
        Some("You can't emote right now.")
    } else if combat {
        Some("You can't emote during combat.")
    } else if swimming {
        Some("You can't emote while swimming.")
    } else if airborne {
        Some("You can't emote in the air.")
    } else if furniture {
        Some("You can't emote while using something.")
    } else {
        None
    }
}

pub struct EmotePlayPlugin;

impl Plugin for EmotePlayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EmotePlay>()
            .init_resource::<EmoteGround>()
            .init_resource::<LocalEmote>()
            .init_resource::<PeerEmotes>()
            .add_systems(Update, drive.in_set(nvf_play::player::BodySet).before(crate::furniture::drive_player))
            .add_systems(Update, puppets.after(nvf_play::player::BodySet))
            .add_systems(Update, ground_audit.run_if(|| std::env::var("NVF_PLAY_TEST_EMOTE_GROUND").is_ok_and(|v| v == "1")))
            .add_systems(Last, report);
    }
}

type Look<'w> = (Res<'w, State<PlayState>>, Option<Res<'w, nvf_play::music::MusicCues>>, Option<Res<'w, nvf_play::combat::CombatStats>>, Res<'w, EmoteBook>);

/// This player's emote: start it when asked, advance it, end it.
#[allow(clippy::too_many_arguments)]
fn drive(
    time: Res<Time>,
    mut asks: MessageReader<EmoteAsk>,
    mut st: ResMut<EmotePlay>,
    mut local: ResMut<LocalEmote>,
    (mut body, mut seat): (ResMut<BodyOverride>, ResMut<nvf_play::furniture::PlayerSeat>),
    (mut player, mut control): (ResMut<nvf_play::player::Player>, ResMut<nvf_play::control::PlayerControl>),
    mut phys: ResMut<nvf_play::phys::PhysState>,
    input: ControlInput,
    (state, cues, stats, book): Look,
    (mut parts, mut models, mut ground): (ResMut<ActorParts>, ModelCtx, ResMut<EmoteGround>),
    mut hud: ResMut<nvf_play::activate::HudMessage>,
) {
    let st = &mut *st;
    let dt = time.delta_secs().min(0.1);
    let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
    let playing = *state.get() == PlayState::Playing;
    let combat = cues.as_ref().is_some_and(|c| c.combat);
    let hits = stats.as_ref().map_or(0, |s| s.player_hits);
    let hit = hits > st.hits;
    st.hits = hits;
    let mut cancel: Option<(&str, bool)> = None;
    for ask in asks.read().copied() {
        match ask {
            EmoteAsk::Cancel => {
                if st.run.is_some() {
                    cancel = Some(("asked", false));
                }
            }
            EmoteAsk::Play { emote, cancel_after, variant: chosen } => {
                let Some(e) = CATALOG.get(emote) else { continue };
                let character = phys.player.as_ref();
                let swimming = character.is_some_and(|c| c.swimming);
                let airborne = character.is_some_and(|c| !c.grounded && !c.swimming);
                // Furniture (not an emote) holding the player.
                let furniture = seat.on.is_some() || (seat.hold.is_some() && st.run.is_none());
                if let Some(why) = refusal(playing, combat, swimming, airborne, furniture, book.available(emote)) {
                    st.refused += 1;
                    info!("emotes: {} refused: {why}", e.key);
                    hud.show(why.to_string());
                    continue;
                }
                // A one-shot: one of its clips installed, at random.
                let (ph, variant) = match e.shape {
                    Shape::Once { .. } => {
                        let have: Vec<usize> = book.installed.get(emote).map_or_else(|| vec![0], |v| v.iter().enumerate().filter(|(_, b)| **b).map(|(i, _)| i).collect());
                        st.seed = st.seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407 ^ (time.elapsed_secs_f64().to_bits()));
                        let pick = have.get((st.seed >> 33) as usize % have.len().max(1)).copied().unwrap_or(0);
                        // A test aid's choice, when that clip is installed.
                        let pick = chosen.map(usize::from).filter(|c| have.contains(c)).unwrap_or(pick);
                        (phase::ONCE, pick as u8)
                    }
                    Shape::Loop { enter, .. } => (if enter.is_some() { phase::ENTER } else { phase::STAY }, 0),
                };
                let Some((path, clip)) = clip_path(emote, variant, ph, 0).and_then(|p| Some((p.clone(), ground.clip(&mut ctx, &p)?))) else {
                    warn!("emotes: {} has no clip to play", e.key);
                    continue;
                };
                // Held where an emote under way had the player, else at the feet.
                // On the ground under the feet (a trace down, not the character's own height).
                let mut feet = player.feet;
                if let Some(hit) = phys.ground_below(feet, 40.0, 200.0) {
                    feet[2] = hit.point[2];
                }
                let mut frame = Frame { pos: feet, heading: player.heading };
                if let Some(old) = st.run.take() {
                    // Another emote over one under way: the old one ends where it is (the
                    // view stays third person for the new one).
                    info!("emotes: end {} (another emote) after {:.2}s; the view stays for the next", CATALOG[old.emote].key, old.age);
                    frame = old.frame;
                }
                if control.pov == Pov::First && st.restore.is_none() {
                    st.restore = Some(Pov::First);
                    control.pov = Pov::Third;
                }
                control.weapon_out = false;
                let stand = ctx.clip(STAND);
                st.serial += 1;
                st.started += 1;
                cancel = None;
                let [x, y, z] = frame.pos;
                info!("emotes: start {} (id {}, {}, variant {variant}): {path} ({:.2}s) at ({x:.0}, {y:.0}, {z:.0}){}", e.key, nvf_play::emotes::Emote::id(emote), phase_name(ph), clip.length(), if st.restore.is_some() { "; the view goes to third person" } else { "" });
                st.run = Some(Run { emote, variant, phase: ph, step: 0, t: 0.0, clip, path, frame, stand, age: 0.0, cancel_at: cancel_after });
            }
        }
    }
    let Some(r) = st.run.as_mut() else {
        if local.state.id != 0 || local.heading.is_some() {
            *local = LocalEmote::default();
        }
        return;
    };
    r.t += dt;
    r.age += dt;
    // What ends it.
    let moved = [Control::Forward, Control::Back, Control::SlideLeft, Control::SlideRight, Control::Jump].into_iter().any(|c| input.just_pressed(c)) || seat.leave;
    seat.leave = false;
    // A menu or a conversation over it (the world stands still) holds it; a loading
    // screen (another place) ends it.
    if cancel.is_none() {
        cancel = if *state.get() == PlayState::Loading {
            Some(("a loading screen", true))
        } else if !playing {
            None
        } else if r.cancel_at.is_some_and(|c| r.age >= c) {
            Some(("test aid", false))
        } else if moved {
            Some(("moved", false))
        } else if control.weapon_out {
            Some(("weapon drawn", true))
        } else if hit {
            Some(("hit", true))
        } else if combat {
            Some(("combat", true))
        } else {
            None
        };
    }
    let key = CATALOG[r.emote].key;
    let mut end = None;
    if let Some((why, fast)) = cancel {
        match r.phase {
            phase::ENTER | phase::STAY => match clip_path(r.emote, u8::from(fast), phase::EXIT, 0).and_then(|p| Some((p.clone(), ground.clip(&mut ctx, &p)?))) {
                Some((path, clip)) if playing => {
                    info!("emotes: {key} exit ({why}{}): {path} ({:.2}s)", if fast { ", fast" } else { "" }, clip.length());
                    (r.phase, r.variant, r.step, r.t, r.clip, r.path) = (phase::EXIT, u8::from(fast), 0, 0.0, clip, path);
                    st.serial += 1;
                }
                _ => end = Some(why),
            },
            phase::ONCE => end = Some(why),
            _ => {}
        }
    }
    // The clip played to its end: the next phase.
    let length = r.clip.length();
    if end.is_none() && r.t >= length && !loops_alone(r.emote, r.phase) {
        let next = match r.phase {
            phase::ENTER => Some((phase::STAY, 0)),
            phase::STAY => Some((phase::STAY, r.step.wrapping_add(1))),
            _ => None,
        };
        match next.and_then(|(ph, step)| clip_path(r.emote, 0, ph, step).and_then(|p| Some((ph, step, p.clone(), ground.clip(&mut ctx, &p)?)))) {
            Some((ph, step, path, clip)) => {
                info!("emotes: {key} {} {}: {path} ({:.2}s)", phase_name(ph), step, clip.length());
                (r.phase, r.step, r.t, r.clip, r.path) = (ph, step, 0.0, clip, path);
                st.serial += 1;
            }
            None => end = Some("done"),
        }
    }
    if let Some(why) = end {
        let r = st.run.take().expect("an emote under way");
        // On the ground where it ends, facing as the body did.
        phys.place_player(r.frame.pos, 60.0, 200.0);
        player.heading = r.frame.heading;
        seat.hold = None;
        body.0 = None;
        if let Some(p) = st.restore.take() {
            control.pov = p;
        }
        let [x, y, z] = r.frame.pos;
        info!("emotes: end {key} ({why}) after {:.2}s at ({x:.0}, {y:.0}, {z:.0}){}", r.age, if control.pov == Pov::First { "; first person again" } else { "" });
        *local = LocalEmote::default();
        return;
    }
    let looped = loops_alone(r.emote, r.phase);
    // `lying`: the third-person camera's pivot goes up as far as the pelvis has tipped
    // over (lying down, push-ups), so the camera stays out of the ground (`player_body.rs`).
    body.0 = Some(BodyClip { clip: r.clip.clone(), looped, serial: st.serial, pos: r.frame.pos, heading: r.frame.heading, lying: true, over: None, under: r.stand.clone() });
    seat.hold = Some(r.frame.pos);
    let t = if looped && length > 1e-3 { r.t % length } else { r.t.min(length) };
    local.state = EmoteState { id: nvf_play::emotes::Emote::id(r.emote), variant: r.variant, phase: r.phase, step: r.step, t };
    local.heading = Some(r.frame.heading);
}

/// The other players' emotes on their puppets.
#[allow(clippy::too_many_arguments)]
fn puppets(
    mut commands: Commands,
    peers: Res<PeerEmotes>,
    mut st: ResMut<EmotePlay>,
    book: Res<EmoteBook>,
    mut rigs: Query<(Entity, &ActorRig, Option<&mut ActorIdle>)>,
    (mut parts, mut models, mut ground): (ResMut<ActorParts>, ModelCtx, ResMut<EmoteGround>),
) {
    let st = &mut *st;
    let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
    for &(reference, e) in &peers.0 {
        // A slot machine's idle is `slots_seat.rs`'s.
        if e.id == 0 || crate::slots_seat::is_slot_emote(e.id) {
            continue;
        }
        let Some((ent, rig, idle)) = rigs.iter_mut().find(|(_, r, _)| r.reference == reference) else { continue };
        let emote = e.id as usize - 1;
        let key = (e.id, e.variant, e.phase, e.step);
        let known = st.puppets.get(&reference);
        let playing_ours = known.is_some_and(|p| idle.as_ref().and_then(|i| i.0.cur.as_ref()).is_some_and(|c| c.serial == p.serial && !c.play.ended()));
        // A new clip, or a loop something else took the puppet's idle from.
        if known.map(|p| p.key) != Some(key) || (!playing_ours && loops_alone(emote, e.phase)) {
            // A one-shot's clip this game does not have: its first one that it has.
            let variant = match book.installed.get(emote) {
                Some(inst) if e.phase == phase::ONCE && !inst.get(e.variant as usize).copied().unwrap_or(false) => inst.iter().position(|b| *b).unwrap_or(0) as u8,
                _ => e.variant,
            };
            let Some((path, clip)) = clip_path(emote, variant, e.phase, e.step).and_then(|p| Some((p.clone(), ground.clip(&mut ctx, &p)?))) else {
                debug!("emotes: puppet {reference:08X}: no clip for emote {} phase {}", e.id, e.phase);
                continue;
            };
            let bound = bind_clip(&rig.skel, clip.clone());
            let passes = if loops_alone(emote, e.phase) { None } else { Some(1) };
            let mut play = IdlePlay::new(IdleKeys::read(clip.start, clip.stop, &clip.text_keys), passes);
            // Seen first under way (joined or came near late): from where its player is.
            let late = known.is_none_or(|p| p.key.0 != e.id) && e.t > 0.5;
            if late {
                play.advance(e.t);
            }
            let serial = match idle {
                Some(mut i) => i.0.start_clip(IdleSource::Script, Arc::new(bound), play),
                None => {
                    let mut i = ActorIdle::default();
                    let s = i.0.start_clip(IdleSource::Script, Arc::new(bound), play);
                    commands.entity(ent).insert(i);
                    s
                }
            };
            if let Some(old) = known.filter(|p| p.key.0 != e.id) {
                info!("emotes: puppet {reference:08X} ended emote {} (another emote)", CATALOG.get(old.key.0 as usize - 1).map_or("?", |c| c.key));
            }
            info!("emotes: puppet {reference:08X} plays {} {}{}: {path}{}", CATALOG.get(emote).map_or("?", |c| c.key), phase_name(e.phase), if e.phase == phase::STAY { format!(" {}", e.step) } else { String::new() }, if late { format!(" from {:.2}s (under way)", e.t) } else { String::new() });
            st.puppets.insert(reference, Puppet { key, serial });
        }
    }
    // Those whose emote ended (or who left).
    let ended: Vec<u32> = st.puppets.keys().copied().filter(|r| !peers.0.iter().any(|(p, e)| p == r && e.id != 0 && !crate::slots_seat::is_slot_emote(e.id))).collect();
    for reference in ended {
        let Some(p) = st.puppets.remove(&reference) else { continue };
        if let Some((_, _, Some(mut idle))) = rigs.iter_mut().find(|(_, r, _)| r.reference == reference) {
            if let Some(c) = idle.0.cur.as_mut().filter(|c| c.serial == p.serial) {
                c.play.stop();
            }
        }
        info!("emotes: puppet {reference:08X} ended emote {}", CATALOG.get(p.key.0 as usize - 1).map_or("?", |c| c.key));
    }
}

fn report(mut exit: MessageReader<AppExit>, st: Res<EmotePlay>, ground: Res<EmoteGround>) {
    if exit.read().next().is_none() || st.started + st.refused == 0 {
        return;
    }
    let moved: Vec<String> = ground.moved.iter().map(|(p, dz)| format!("{} {dz:+.1}", p.rsplit('\\').next().unwrap_or(p))).collect();
    info!("emotes summary: {} started, {} refused; {} clips set on the ground ({} moved: {})", st.started, st.refused, ground.clips.len(), moved.len(), moved.join(", "));
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvf_play::emotes::by_key;

    #[test]
    fn clips_of_each_phase() {
        let sit = by_key("sit").unwrap();
        assert_eq!(clip_path(sit, 0, phase::ENTER, 0).unwrap(), vanilla("sitfloorenter"));
        assert_eq!(clip_path(sit, 0, phase::STAY, 5).unwrap(), vanilla("sitfloordynamicidle"));
        assert_eq!(clip_path(sit, 0, phase::EXIT, 0).unwrap(), vanilla("sitfloorexit"));
        assert_eq!(clip_path(sit, 1, phase::EXIT, 0).unwrap(), vanilla("sitfloorexitfast"));
        let push = by_key("pushups").unwrap();
        assert_eq!(clip_path(push, 0, phase::ENTER, 0), None);
        assert_eq!(clip_path(push, 1, phase::EXIT, 0).unwrap(), vanilla("pushupsgetup"), "no fast exit: the exit");
        let warm = by_key("warmup").unwrap();
        assert_eq!(clip_path(warm, 0, phase::STAY, 4).unwrap(), vanilla("ms09specialidle_warmingup02"), "in turn");
        assert!(loops_alone(sit, phase::STAY) && !loops_alone(warm, phase::STAY) && !loops_alone(sit, phase::ENTER));
        let wave = by_key("wave").unwrap();
        assert_eq!(clip_path(wave, 3, phase::ONCE, 0).unwrap(), "meshes\\AG_WaveShort.kf");
        assert_eq!(clip_path(wave, 9, phase::ONCE, 0), None);
    }

    #[test]
    fn refusals_in_order() {
        assert_eq!(refusal(true, false, false, false, false, true), None);
        assert_eq!(refusal(true, true, true, false, false, true), Some("You can't emote during combat."));
        assert_eq!(refusal(false, false, false, false, false, true), Some("You can't emote right now."));
        assert_eq!(refusal(true, false, false, true, false, true), Some("You can't emote in the air."));
        assert_eq!(refusal(true, false, false, false, false, false), Some("That emote is not installed."));
    }
}
