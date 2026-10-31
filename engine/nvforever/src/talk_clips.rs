//! Speakers without a face rig light up as they talk (docs/spec-notes/lipsync.md §6.1):
//! a creature whose skeleton folder has a `talking.kf` plays its `Talking` sequence while
//! it says a line, the sequence's time following the line's `.lip`.
//!
//! - **What the data has** [observed: `fnvtool nif decode`, lipsync.md §6.1]: eight
//!   creature folders carry a `talking.kf` (Securitron, Mr. House's screen, Mr. House in
//!   his pod, protectron, robobrain, Festus, Liberty Prime, super mutants). Their
//!   `Talking` sequences are short (0.17 s for five of them), clamped, and drive the
//!   speaker's lights: the Securitron's chest light `##VoiceBox_talk:0` (in
//!   `nvsecuritronvoicebox.nif`, which Victor and the Yes Man and street Securitrons
//!   wear) and its face `Screen01:0`, Mr. House's `##TalkButtons:0`, the protectron's
//!   `Dome`. The idles hold those lights where they rest: the Securitron's `mtidle.kf`
//!   keeps the chest light at 0 (dark) with priority 0, `Talking` drives it with 85.
//! - **Time from the voice** [hypothesis]: Mr. House's `##TalkButtons:0` rises steadily
//!   from 0.04 to 1.0 over the 0.17 s, the protectron's, robobrain's and Liberty Prime's
//!   tracks are two-key ramps: the sequence reads as a scale from quiet to loud rather
//!   than a loop. So the sequence's time is the line's loudness now, taken from its
//!   `.lip`: the largest mouth-shape weight of the frame (0 to 1 of the sequence)
//!   [engine choice: the `.lip` stands for the sound's loudness; observer-checks.md asks
//!   what the game reads]. A line whose sound has no `.lip` holds the middle
//!   ([`NO_LIP_LEVEL`]); a response with no sound file does not light the speaker
//!   (`nvf_play::voice` reports no sound path for it).
//! - **What plays**: the sequence's material tracks and its bone tracks, without the
//!   accumulation root and `Bip01 NonAccum` (where a walking body is carried)
//!   [design], on blend layer [`TALK_LAYER`] over everything else; `locomotion::drive`
//!   lays it on. Between lines nothing plays: the idle's own values come back.
//!
//! Log lines: `talk clip: <folder>\talking.kf ...` once per folder, `talk clip: <who>
//! says <path> ...` when a line starts, `talk clip: <who> line over ...` when it ends, a
//! `talk clip summary:` at the end.

use bevy::prelude::*;
use fnv_sim::FormId;
use fnv_voice::lip::{Lip, PHONEMES};
use nvf_assets::actor::{bind_clip, ActorCtx, ActorParts};
use nvf_assets::anim_play::{BlendPlayer, BoundClip, Clip, Layer};
use nvf_assets::model::ModelCtx;
use nvf_play::voice::VoiceState;
use std::collections::HashMap;
use std::sync::Arc;

use crate::limbs::ActorRig;
use crate::locomotion::Locomotion;

/// The blend layer of the `Talking` sequence (`anim_play::MAX_LAYERS`).
pub const TALK_LAYER: usize = 7;

/// The level a line whose sound has no `.lip` holds (0 to 1 of the sequence) [engine
/// choice]. A response without a sound file plays nothing.
pub const NO_LIP_LEVEL: f32 = 0.5;

pub struct TalkClipsPlugin;

impl Plugin for TalkClipsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TalkStats>()
            .init_resource::<Folders>()
            .add_systems(Update, (attach, levels).chain().before(crate::locomotion::drive).after(nvf_play::PlaySet::Scripts))
            .add_systems(Last, report);
    }
}

/// On a creature that has a `Talking` sequence.
#[derive(Component)]
pub struct TalkClip {
    clip: Arc<BoundClip>,
    /// The level now (0 to 1 of the sequence) while a line is said.
    level: Option<f32>,
    /// The line being said: its sound path, start, `.lip`, and frames counted with the
    /// sum and peak of their levels.
    line: Option<Line>,
}

struct Line {
    path: String,
    started_at: f64,
    lip: Option<Arc<Lip>>,
    frames: u32,
    sum: f32,
    peak: f32,
}

impl TalkClip {
    /// The sequence as [`TALK_LAYER`] while a line is said, else nothing there.
    pub fn layers(&self, player: &mut BlendPlayer) {
        player.layers[TALK_LAYER] = self.level.map(|l| Layer { clip: self.clip.clone(), phase: l.clamp(0.0, 0.999), weight: 1.0 });
    }
}

/// On a creature looked at that has no `Talking` sequence.
#[derive(Component)]
pub struct NoTalkClip;

/// `Talking` by skeleton folder, without the tracks that carry the body (`None`: the
/// folder has none).
#[derive(Resource, Default)]
struct Folders(HashMap<String, Option<Arc<Clip>>>);

#[derive(Resource, Default)]
struct TalkStats {
    lines: u64,
    without_lip: u64,
    speakers: Vec<u32>,
}

/// The folder of a skeleton path (`creatures\nvsecuritron\skeleton.nif` → `meshes\creatures\nvsecuritron`).
fn folder_of(skeleton: &str) -> String {
    let s = fnv_vfs::normalize(&fnv_actor::resolve::mesh_path(skeleton));
    s.rfind('\\').map_or(String::new(), |i| s[..i].to_string())
}

/// `clip` without the tracks of its accumulation root and of `Bip01 NonAccum`.
pub fn without_carriers(clip: &Clip) -> Clip {
    let mut c = clip.clone();
    let carries = |n: &str| n.eq_ignore_ascii_case("Bip01 NonAccum") || c.accum_root.as_deref().is_some_and(|r| r.eq_ignore_ascii_case(n));
    let keep: Vec<bool> = c.tracks.iter().map(|(n, _)| !carries(n)).collect();
    let mut k = keep.iter();
    c.tracks.retain(|_| *k.next().unwrap_or(&true));
    let mut k = keep.iter();
    c.priorities.retain(|_| *k.next().unwrap_or(&true));
    c
}

/// The level of a line `t` seconds into its sound: the largest mouth-shape weight of its
/// `.lip` then, 0 before the sound starts and after its last frame.
pub fn level_at(lip: &Lip, t: f64) -> f32 {
    let w = if t < 0.0 { None } else { lip.sample(t) };
    w.map_or(0.0, |w| w[..PHONEMES].iter().fold(0.0f32, |a, &x| a.max(x)).clamp(0.0, 1.0))
}

type Fresh<'w> = (Entity, &'w ActorRig);
type Unseen = (With<Locomotion>, With<BlendPlayer>, Without<TalkClip>, Without<NoTalkClip>);

/// Give every animated creature with a `Talking` sequence its clip, once.
fn attach(mut commands: Commands, fresh: Query<Fresh, Unseen>, mut folders: ResMut<Folders>, mut parts: ResMut<ActorParts>, mut models: ModelCtx) {
    if fresh.is_empty() {
        return;
    }
    let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
    for (e, rig) in &fresh {
        let folder = folder_of(&rig.skeleton);
        let clip = folder.starts_with("meshes\\creatures\\").then(|| {
            folders
                .0
                .entry(folder.clone())
                .or_insert_with(|| {
                    let path = format!("{folder}\\talking.kf");
                    let c = ctx.clip(&path).map(|c| Arc::new(without_carriers(&c)));
                    if let Some(c) = &c {
                        let mut shapes: Vec<String> = c.materials.iter().map(|m| m.shape.to_ascii_lowercase()).collect();
                        shapes.dedup();
                        info!("talk clip: {path} ({}, {:.2} s): {} material tracks on {}; {} bone tracks", c.name, c.length(), c.materials.len(), if shapes.is_empty() { "none".to_string() } else { shapes.join(", ") }, c.tracks.len());
                    }
                    c
                })
                .clone()
        });
        match clip.flatten() {
            Some(c) => {
                commands.entity(e).insert(TalkClip { clip: Arc::new(bind_clip(&rig.skel, c)), level: None, line: None });
            }
            None => {
                commands.entity(e).insert(NoTalkClip);
            }
        }
    }
}

/// Each creature's level from the line it says now; log lines starting and ending.
fn levels(real: Res<Time<Real>>, voice: Res<VoiceState>, script: Option<Res<nvf_play::script::ScriptState>>, mut stats: ResMut<TalkStats>, mut q: Query<(&ActorRig, &mut TalkClip)>) {
    let now = real.elapsed_secs_f64();
    let name = |id: u32| script.as_ref().and_then(|s| s.rt.as_ref()).map_or_else(|| format!("{id:08X}"), |rt| format!("{} {id:08X}", rt.game.data.name(FormId(id))));
    for (rig, mut tc) in &mut q {
        let said = voice.spoken().find(|s| s.speaker.0 == rig.reference).and_then(|s| Some((s.path?.to_string(), s.started_at, s.position(now))));
        let same = matches!((&tc.line, &said), (Some(l), Some((p, t, _))) if l.path == *p && l.started_at == *t);
        if !same {
            if let Some(l) = tc.line.take() {
                let mean = if l.frames > 0 { l.sum / l.frames as f32 } else { 0.0 };
                info!("talk clip: {} line over: {} frames, level mean {mean:.2}, peak {:.2}", name(rig.reference), l.frames, l.peak);
            }
            if let Some((path, started_at, _)) = &said {
                let lip_path = fnv_voice::lip::lip_path(path);
                let lip = voice.bank.as_ref().and_then(|b| b.read(&lip_path)).and_then(|bytes| Lip::parse(&bytes).ok()).map(Arc::new);
                stats.lines += 1;
                stats.without_lip += u64::from(lip.is_none());
                if !stats.speakers.contains(&rig.reference) {
                    stats.speakers.push(rig.reference);
                }
                info!(
                    "talk clip: {} says {path}: {} from its .lip{}",
                    name(rig.reference),
                    tc.clip.clip.name,
                    match &lip {
                        Some(l) => format!(" ({} frames)", l.frames.len()),
                        None => format!(": none, held at {NO_LIP_LEVEL}"),
                    }
                );
                tc.line = Some(Line { path: path.clone(), started_at: *started_at, lip, frames: 0, sum: 0.0, peak: 0.0 });
            }
        }
        tc.level = match (&mut tc.line, &said) {
            (Some(l), Some((_, _, t))) => {
                let v = l.lip.as_ref().map_or(NO_LIP_LEVEL, |lip| level_at(lip, *t));
                l.frames += 1;
                l.sum += v;
                l.peak = l.peak.max(v);
                Some(v)
            }
            _ => None,
        };
    }
}

fn report(mut exit: MessageReader<AppExit>, stats: Res<TalkStats>) {
    if exit.read().next().is_none() {
        return;
    }
    info!("talk clip summary: {} lines by {} speakers with a Talking sequence, {} without a .lip", stats.lines, stats.speakers.len(), stats.without_lip);
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_voice::lip::CHANNELS;

    #[test]
    fn the_level_is_the_widest_mouth_shape_of_the_frame() {
        let mut a = [0.0; CHANNELS];
        a[0] = 0.25;
        a[3] = 0.75;
        a[PHONEMES] = 1.0; // a blink, not a mouth shape
        let lip = Lip { version: 1, flag: 1, start_frame: 0, frames: vec![a, [0.0; CHANNELS]] };
        assert!((level_at(&lip, 0.0) - 0.75).abs() < 1e-6);
        assert_eq!(level_at(&lip, -0.1), 0.0);
        assert_eq!(level_at(&lip, 10.0), 0.0);
    }

    #[test]
    fn the_body_carriers_are_left_out() {
        let mut c = Clip::from_controllers(Vec::new(), Vec::new(), Vec::new());
        use fnv_nif::anim::{Channel, TransformTrack};
        let t = Arc::new(TransformTrack { translation: Channel::Static(None), rotation: Channel::Static(None), euler: None, scale: Channel::Static(None) });
        c.tracks = vec![("Bip01".into(), t.clone()), ("Bip01 NonAccum".into(), t.clone()), ("Bip01 Speaker".into(), t)];
        c.priorities = vec![10, 20, 97];
        c.accum_root = Some("Bip01".into());
        let k = without_carriers(&c);
        assert_eq!(k.tracks.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["Bip01 Speaker"]);
        assert_eq!(k.priorities, [97]);
    }

    #[test]
    fn folders_of_skeletons() {
        assert_eq!(folder_of(r"Creatures\NVSecuritron\Skeleton.NIF"), r"meshes\creatures\nvsecuritron");
    }
}
