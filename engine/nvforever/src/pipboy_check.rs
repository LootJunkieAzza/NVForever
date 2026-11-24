//! The Pip-Boy checked on a third-person body (the co-op fork's own [design: owner
//! 2026-11-03]): the player's own body and the co-op puppets of other players raise the
//! left arm and look at it while that player has the Pip-Boy up, so everyone sees who is
//! checking theirs.
//!
//! - **The clips** are the game's own third-person idles [observed: FalloutNV.esm IDLE
//!   `PipboyManipulate` 0x022C13 and `PipboyExamine` 0x022C11, under the same parent;
//!   `meshes\characters\_male\idleanims\pipboymanipulate.kf` (12.67 s: "AccessUp" at
//!   0.37 s, the first "PipBoyUi" at 1.90 s, "AccessDown" at 11.57 s) and
//!   `pipboyexamine.kf` (7.37 s: "AccessUp" at 0.03 s, the first "PipBoyStatic" at
//!   0.60 s, "AccessDown" at 6.73 s)]. The full Pip-Boy (Tab) plays Manipulate, the
//!   glance (G) Examine [design: the full check works the buttons, the glance only
//!   looks].
//! - **Held while open** [design]: the clip plays from its start, then goes round its
//!   raised part (from its first "PipBoyUi" or "PipBoyStatic" key to its "AccessDown"
//!   key, [`CheckKeys`]) while the Pip-Boy stays up; put away, it goes on from
//!   "AccessDown" to its end, the arm lowered, and fades out. Put away before the arm was
//!   up, it fades out where it is.
//! - **Upper body only**: the clips drive the whole body (priority 15, the head 95)
//!   [observed]; the legs, the pelvis and the root are left to the walk, so a player who
//!   glances while moving keeps walking (as the stimpak idle is, `npc_stimpaks.rs`).
//! - The co-op state says which: `nvf_coop::net::PlayerState::pipboy`
//!   (`nvf_play::pipboy::glance::Check`).

use crate::limbs::ActorRig;
use bevy::prelude::*;
use nvf_assets::actor::{bind_clip, ActorCtx, ActorParts};
use nvf_assets::anim_play::{BlendPlayer, BoundClip, Clip, Layer};
use nvf_assets::model::ModelCtx;
use nvf_play::pipboy::glance::Check;
use std::collections::HashMap;
use std::sync::Arc;

/// The full Pip-Boy's and the glance's clips, in the skeleton's folder.
pub const FULL_CLIP: &str = "idleanims\\pipboymanipulate.kf";
pub const GLANCE_CLIP: &str = "idleanims\\pipboyexamine.kf";

/// How long the check fades in and out, seconds [observed: the clips' "Blend: 15" key;
/// hypothesis: frames at 30 a second, as for the first-person clip].
pub const BLEND: f32 = 0.5;

/// The clip's file for a check.
pub fn clip_of(full: bool) -> &'static str {
    if full {
        FULL_CLIP
    } else {
        GLANCE_CLIP
    }
}

/// `clip` with its lower body left out, its other tracks above the walk and the weapon's
/// stance (`npc_stimpaks::UPPER_PRIORITY`), and no root of its own.
pub fn upper_clip(clip: &Clip) -> Clip {
    let mut c = clip.clone();
    let keep: Vec<bool> = c.tracks.iter().map(|(n, _)| !crate::npc_stimpaks::lower_body(n)).collect();
    let mut k = keep.iter();
    c.tracks.retain(|_| *k.next().unwrap_or(&true));
    let mut k = keep.iter();
    c.priorities.retain(|_| *k.next().unwrap_or(&true));
    for p in c.priorities.iter_mut() {
        *p = (*p).max(crate::npc_stimpaks::UPPER_PRIORITY);
    }
    c.accum_root = None;
    c
}

/// Where a check clip's arm is up and where it goes down ([`CheckPlay`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CheckKeys {
    pub len: f32,
    /// The raised part's start: the first "PipBoyUi" or "PipBoyStatic" sound key.
    pub up: f32,
    /// The "AccessDown" sound key: the arm goes down from here.
    pub down: f32,
}

impl CheckKeys {
    /// From the clip's text keys; without them a quarter and nine tenths of its length
    /// [design].
    pub fn read(len: f32, keys: &[(f32, String)]) -> CheckKeys {
        let has = |t: &str, k: &str| t.to_ascii_lowercase().contains(k);
        let up = keys.iter().find(|(_, t)| has(t, "pipboyui") || has(t, "pipboystatic")).map_or(len * 0.25, |k| k.0);
        let down = keys.iter().find(|(_, t)| has(t, "accessdown")).map_or(len * 0.9, |k| k.0);
        if up < down && down <= len && up > 0.0 {
            CheckKeys { len, up, down }
        } else {
            CheckKeys { len, up: len * 0.25, down: len * 0.9 }
        }
    }
}

/// A check playing on a body.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CheckPlay {
    /// The check whose clip plays (`None`: nothing).
    pub kind: Check,
    /// Seconds into its clip.
    pub t: f32,
    /// How far it shows, 0 to 1 (before easing).
    pub weight: f32,
    /// Put away: going down.
    pub closing: bool,
    /// Seconds since it was put away (for the log).
    pub out: f32,
}

impl CheckPlay {
    /// Whether the weapon in the hand is put out of sight: while the Pip-Boy is up, from
    /// the check's start until it is put away [design, owner playtest 2026-11-06: the other
    /// player's weapon stayed in hand while they looked at their Pip-Boy; it is to be
    /// hidden while the Pip-Boy is up and come back when it goes down].
    pub fn hides_weapon(&self) -> bool {
        self.kind != Check::None && !self.closing
    }

    /// The check whose clip is wanted now: the one going down until it is gone, else what
    /// the player does.
    pub fn clip_for(&self, want: Check) -> Check {
        if self.kind != Check::None && (self.closing || want == Check::None) {
            self.kind
        } else {
            want
        }
    }

    /// Move on by `dt` with the player doing `want`; `keys` are those of
    /// [`Self::clip_for`]'s clip. Returns the clip's time and its eased weight while it
    /// shows.
    pub fn step(&mut self, want: Check, dt: f32, keys: CheckKeys) -> Option<(f32, f32)> {
        let kind = self.clip_for(want);
        if kind == Check::None {
            *self = CheckPlay::default();
            return None;
        }
        if kind != self.kind {
            // The other check while the arm is up: the new clip from where its arm is up.
            let up_already = self.kind != Check::None && !self.closing && self.weight >= 0.5;
            *self = CheckPlay { kind, t: if up_already { keys.up } else { 0.0 }, weight: if up_already { self.weight } else { 0.0 }, closing: false, out: 0.0 };
        }
        if want == kind {
            if self.closing {
                (self.closing, self.out) = (false, 0.0);
                if self.t >= keys.down {
                    self.t = keys.up;
                }
            }
            self.t += dt;
            if self.t >= keys.down {
                let span = (keys.down - keys.up).max(0.01);
                self.t = keys.up + (self.t - keys.down) % span;
            }
            self.weight = (self.weight + dt / BLEND).min(1.0);
        } else {
            if !self.closing {
                self.closing = true;
                if self.t >= keys.up {
                    self.t = self.t.max(keys.down);
                }
            }
            self.out += dt;
            if self.t < keys.up {
                // Put away before the arm was up: it fades out where it is. Its time
                // stays put, so it never runs on into the raised part with the weight
                // left where it was (closed 1.8 s into the full check, 0.1 s before its
                // arm is up, the arm stayed raised the clip's next 10.8 s; owner's play
                // 2026-11-13).
                self.weight -= dt / BLEND;
            } else {
                self.t += dt;
                self.weight = self.weight.min((keys.len - self.t) / BLEND);
            }
            if self.weight <= 0.0 || self.t >= keys.len {
                *self = CheckPlay::default();
                return None;
            }
        }
        let w = self.weight.clamp(0.0, 1.0);
        Some((self.t.min(keys.len), w * w * (3.0 - 2.0 * w)))
    }
}

/// This game's player's check on the third-person body (`player_body.rs`).
#[derive(Resource, Default)]
pub struct PlayerCheck(pub CheckPlay);

/// What each other player's Pip-Boy does, by its puppet's reference (written by
/// `coop.rs`).
#[derive(Resource, Default)]
pub struct PuppetChecks(pub HashMap<u32, Check>);

/// A puppet's check and the layer it gives (`locomotion::drive` lays it over the rest).
#[derive(Component, Default)]
pub struct CheckAnim {
    pub play: CheckPlay,
    pub layer: Option<Layer>,
    /// Its weapon in the hand was put out of sight ([`hide_weapons`]).
    pub weapon_hidden: bool,
}

impl CheckAnim {
    /// Over the idle layers: the first that is free.
    pub fn layers(&self, player: &mut BlendPlayer) {
        let Some(l) = self.layer.clone() else { return };
        let i = crate::idles::IDLE_LAYER;
        let slot = if player.layers[i].is_none() { i } else { i + 1 };
        player.layers[slot] = Some(l);
    }
}

/// The check clips bound to each skeleton (`None`: none to play).
#[derive(Resource, Default)]
pub struct PuppetClips(HashMap<(String, bool), Option<CheckClip>>);

/// A check clip bound to a skeleton, with its keys.
type CheckClip = (Arc<BoundClip>, CheckKeys);

pub struct PipBoyCheckPlugin;

impl Plugin for PipBoyCheckPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerCheck>().init_resource::<PuppetChecks>().init_resource::<PuppetClips>().add_systems(Update, (puppets, hide_weapons).chain().before(crate::locomotion::drive));
    }
}

/// Step each puppet's check and make its layer.
fn puppets(
    mut commands: Commands,
    time: Res<Time>,
    checks: Res<PuppetChecks>,
    mut clips: ResMut<PuppetClips>,
    mut parts: ResMut<ActorParts>,
    mut models: ModelCtx,
    mut actors: Query<(Entity, &ActorRig, Option<&mut CheckAnim>), With<crate::coop::Puppet>>,
) {
    let dt = time.delta_secs().min(0.1);
    let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
    for (e, rig, anim) in &mut actors {
        let want = checks.0.get(&rig.reference).copied().unwrap_or_default();
        let Some(mut anim) = anim else {
            if want != Check::None {
                commands.entity(e).insert(CheckAnim::default());
            }
            continue;
        };
        let kind = anim.play.clip_for(want);
        let found = match kind {
            Check::None => None,
            k => {
                let full = k == Check::Full;
                clips
                    .0
                    .entry((rig.skeleton.clone(), full))
                    .or_insert_with(|| {
                        let s = fnv_vfs::normalize(&rig.skeleton);
                        let folder = s.rfind('\\').map_or("", |i| &s[..i]).to_string();
                        let path = format!("{folder}\\{}", clip_of(full));
                        let clip = ctx.clip(&path)?;
                        let keys = CheckKeys::read(clip.length(), &clip.text_keys);
                        let bound = crate::npc_stimpaks::upper_body(bind_clip(&rig.skel, clip), &rig.skel.names);
                        if bound.tracks.is_empty() {
                            warn!("pipboy check: {path} drives no node of {}", rig.skeleton);
                            return None;
                        }
                        info!("pipboy check: {path} on {} ({:.2} s, up {:.2} s, down {:.2} s)", rig.skeleton, keys.len, keys.up, keys.down);
                        Some((Arc::new(bound), keys))
                    })
                    .clone()
            }
        };
        anim.layer = match found {
            Some((clip, keys)) => anim.play.step(want, dt, keys).map(|(t, w)| Layer { clip, phase: (t / keys.len.max(1e-3)).min(0.999), weight: w }),
            None => {
                anim.play = CheckPlay::default();
                None
            }
        };
    }
}

/// A puppet's weapon in the hand is out of sight while its player has the Pip-Boy up
/// ([`CheckPlay::hides_weapon`]): its `Weapon` node is hidden, and shown again after.
/// A holstered weapon stays seen.
fn hide_weapons(mut actors: Query<(&ActorRig, &mut CheckAnim, &crate::weapons::WeaponHold, &BlendPlayer), With<crate::coop::Puppet>>, mut visibility: Query<&mut Visibility>) {
    for (rig, mut anim, hold, player) in &mut actors {
        let hide = anim.play.hides_weapon() && hold.in_hand();
        if hide == anim.weapon_hidden {
            continue;
        }
        let Some(&node) = player.nodes.get(hold.stance.weapon_node) else { continue };
        if let Ok(mut v) = visibility.get_mut(node) {
            *v = if hide { Visibility::Hidden } else { Visibility::Inherited };
            anim.weapon_hidden = hide;
            info!("pipboy check: player {:08X}'s weapon {}", rig.reference, if hide { "put out of sight (Pip-Boy up)" } else { "seen again" });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn examine() -> CheckKeys {
        let keys = vec![
            (0.0, "start".to_string()),
            (0.0333, "Blend: 15".to_string()),
            (0.0333, "Sound: NPCIdlePipBoyAccessUp".to_string()),
            (0.6, "Sound: NPCIdlePipBoyStatic".to_string()),
            (2.67, "Sound: NPCIdlePipBoyStatic".to_string()),
            (6.73, "Sound: NPCIdlePipBoyAccessDown".to_string()),
            (7.37, "end".to_string()),
        ];
        CheckKeys::read(7.37, &keys)
    }

    #[test]
    fn the_keys_are_read_from_the_clips_sounds() {
        assert_eq!(examine(), CheckKeys { len: 7.37, up: 0.6, down: 6.73 });
        let manipulate = vec![(0.37, "Sound: NPCIdlePipBoyAccessUp".to_string()), (1.9, "Sound: NPCIdlePipBoyUi".to_string()), (11.57, "Sound: NPCIdlePipBoyAccessDown".to_string())];
        assert_eq!(CheckKeys::read(12.67, &manipulate), CheckKeys { len: 12.67, up: 1.9, down: 11.57 });
        // Nothing to go by: a quarter and nine tenths.
        assert_eq!(CheckKeys::read(4.0, &[]), CheckKeys { len: 4.0, up: 1.0, down: 3.6 });
    }

    #[test]
    fn a_check_comes_up_goes_round_its_raised_part_and_goes_down() {
        let k = examine();
        let mut p = CheckPlay::default();
        assert_eq!(p.step(Check::None, 0.1, k), None);
        let (t, w) = p.step(Check::Glance, 0.1, k).unwrap();
        assert!((t - 0.1).abs() < 1e-5 && w > 0.0 && w < 1.0);
        // Held: round the raised part, at full weight.
        for _ in 0..200 {
            let (t, w) = p.step(Check::Glance, 0.1, k).unwrap();
            assert!(t < k.down, "{t}");
            if p.t > 1.0 {
                assert!((w - 1.0).abs() < 1e-5);
            }
        }
        assert!(p.t >= k.up);
        // Put away: from AccessDown to the end, fading.
        let (t, _) = p.step(Check::None, 0.1, k).unwrap();
        assert!(t >= k.down);
        let mut n = 0;
        while p.step(Check::None, 0.1, k).is_some() {
            n += 1;
            assert!(n < 100);
        }
        assert_eq!(p, CheckPlay::default());
    }

    #[test]
    fn the_weapon_is_hidden_while_the_pipboy_is_up() {
        let k = examine();
        let mut p = CheckPlay::default();
        assert!(!p.hides_weapon());
        p.step(Check::Glance, 0.1, k);
        assert!(p.hides_weapon(), "from the start");
        for _ in 0..50 {
            p.step(Check::Glance, 0.1, k);
        }
        assert!(p.hides_weapon(), "while up");
        p.step(Check::None, 0.1, k);
        assert!(!p.hides_weapon(), "back as soon as it is put away");
    }

    #[test]
    fn put_away_before_the_arm_is_up_it_fades_where_it_is() {
        let k = examine();
        let mut p = CheckPlay::default();
        p.step(Check::Glance, 0.2, k);
        let (t, _) = p.step(Check::None, 0.05, k).unwrap();
        assert!(t < k.up, "{t}");
        assert_eq!(p.clip_for(Check::None), Check::Glance);
        for _ in 0..20 {
            p.step(Check::None, 0.05, k);
        }
        assert_eq!(p.kind, Check::None);
    }

    /// Closed at any moment, the check is gone within its out-blend: the fade where it
    /// is ([`BLEND`]) before the arm is up, else the clip from "AccessDown" to its end.
    /// It never plays on into the raised part with the Pip-Boy down (owner's play
    /// 2026-11-13: the full check closed 1.8 s in, 0.1 s before its arm was up, held the
    /// arm raised 10.8 s more).
    #[test]
    fn closed_at_any_moment_the_check_is_gone_within_its_out_blend() {
        let manipulate = CheckKeys { len: 12.67, up: 1.9, down: 11.57 };
        let dt = 1.0 / 60.0;
        for (want, k) in [(Check::Glance, examine()), (Check::Full, manipulate)] {
            let out = BLEND.max(k.len - k.down) + 2.0 * dt;
            let mut open = 0.0;
            while open < k.len + 3.0 {
                let mut p = CheckPlay::default();
                let mut t = 0.0;
                while t < open {
                    p.step(want, dt, k);
                    t += dt;
                }
                let (mut gone, mut last_w) = (0.0, f32::MAX);
                while let Some((at, w)) = p.step(Check::None, dt, k) {
                    gone += dt;
                    assert!(gone <= out, "{want:?} closed at {open:.2} s: still shown {gone:.2} s later (clip at {at:.2} s, weight {w:.2})");
                    assert!(w <= last_w + 1e-5, "{want:?} closed at {open:.2} s: the weight rose to {w:.2}");
                    assert!(at < k.up || at >= k.down, "{want:?} closed at {open:.2} s: the clip plays its raised part at {at:.2} s");
                    last_w = w;
                }
                assert_eq!(p, CheckPlay::default());
                open += 0.05;
            }
        }
    }

    #[test]
    fn from_the_glance_to_the_full_pipboy_the_arm_stays_up() {
        let k = examine();
        let mut p = CheckPlay::default();
        for _ in 0..30 {
            p.step(Check::Glance, 0.1, k);
        }
        let (t, w) = p.step(Check::Full, 0.1, k).unwrap();
        assert_eq!(p.kind, Check::Full);
        assert!(t >= k.up && w > 0.9, "{t} {w}");
        // And opened again while going down: it goes round the raised part again.
        p.step(Check::None, 0.1, k);
        assert!(p.closing);
        let (t, _) = p.step(Check::Full, 0.1, k).unwrap();
        assert!(!p.closing && t < k.down);
    }
}
