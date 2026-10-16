//! How actors look while they fight (docs/design/combat-ai.md §9): NPCs aim at their
//! target and play an attack clip with every shot and blow; creatures play a bite or
//! swipe with every blow. The decisions are `npc_combat.rs`'s ([`PlayAttack`]); this
//! module finds the clips, and `locomotion::drive` lays them over what the actor plays
//! otherwise ([`CombatPose::layers`]).
//!
//! - **Aiming.** An NPC in a fight with its weapon drawn bends to its target's height:
//!   its stance's level aim clip (`2hraim.kf`, played by `weapons.rs`) mixed with the
//!   up or down form (`2hraimup.kf`, `2hraimdown.kf`) by the pitch from its eyes to the
//!   target's chest, as the player's body does (`fnv_actor::player_clips::aim_weights`,
//!   docs/spec-notes/animation.md §9.6: the forms are the poses at ±90°) [hypothesis, as
//!   there].
//! - **Shots and blows of NPCs.** The weapon's attack clip, chosen as the player's is
//!   (`player_clips::attack_stem` of the WEAP attack animation, then `attack_clips`:
//!   `2hrattack3.kf`, `1hpattackright.kf`, `h2hattackright_a.kf` …), in the skeleton's
//!   folder, with its own up or down form at the aim's weight (§9.6: "an attack plays
//!   with its own up and down forms"). Fists are `h2h`.
//! - **Creatures' blows.** The creature's own `h2hattack….kf` clips in its skeleton's
//!   folder (the dog's `h2hattackleft.kf` and `h2hattackright.kf`, the radscorpion's
//!   `h2hattackleft_mid.kf` …) [observed: the base meshes archive, 2026-10-11], power and
//!   forward attacks left out, taken in turn [design].
//! - **When a blow lands**: at the clip's `Hit` text key [documented:
//!   NiTextKeyExtraData; observed: 931 `hit` keys in the clips, animation.md §3.2]; the
//!   key's time is told to `npc_combat` ([`AttackTimes`]), which delays the blow's damage
//!   by it. Shots leave at the start of their clip [design].
//! - **Automatic fire with a looping clip** (`2hhattackspin.kf` of the minigun, flamer and
//!   gatling laser: `Fire` and `Loop` keys, animation.md §9.8): the first shot starts the
//!   clip at its `Fire` key (NPCs do not spin up first [design]), the shots that follow
//!   while it runs do not start it again, and it goes round from `Fire` to `Loop` while
//!   they come no more than [`LOOP_HOLD`] apart, then plays out from where it is, as the
//!   player's body does. Started again at every shot, it jerked the arms and shoulders
//!   back to its start many times a second [observed in NVForever: owner's playtest
//!   2026-10-16, Cook-Cook's flamer].
//! - While an attack clip plays, the actor's body block keeps the place it had when the
//!   attack began (`nvf_play::blocking::ActorBlocks::attacking`): a dog's lunge shoved
//!   the player back 40 units each bite before [design; owner 2026-10-11].
//! - An attack clip is laid on blend layers 4 and 5 (the idles' layers: an idle stops for
//!   the attack) over the aim, by track priority (attack clips 55, aim 50,
//!   animation.md §9.6); the aim's up or down form takes layer 3 when the weapon's draw
//!   cycle is not fading there [design].

use crate::actors::ActorWorld;
use crate::limbs::ActorRig;
use crate::locomotion::Dying;
use crate::npc_combat::Fighters;
use crate::weapons::WeaponHold;
use bevy::prelude::*;
use fnv_actor::player_clips::{aim_variant, aim_weights, attack_clips, attack_stem, Aim, ClipKeys, Stance};
use fnv_actor::weapon::{AnimType, Stage};
use fnv_sim::FormId;
use nvf_assets::actor::{bind_clip, ActorCtx, ActorParts, PreparedSkeleton};
use nvf_assets::anim_play::{BlendPlayer, BoundClip, Layer};
use nvf_assets::model::ModelCtx;
use nvf_play::combat::Combat;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// The layer an attack clip plays on, and its up or down form on the next.
pub const ATTACK_LAYER: usize = 4;
/// The layer of the aim's up or down form.
pub const AIM_VARIANT_LAYER: usize = 3;
/// Seconds an attack clip fades in and out [design].
const FADE_IN: f32 = 0.06;
const FADE_OUT: f32 = 0.15;
/// The up or down form's weight is capped here (the level clip keeps a share) [design].
const MAX_VARIANT: f32 = 0.95;
/// A blow lands this long into its clip when the clip has no `Hit` key, seconds [design].
pub const DEFAULT_HIT: f32 = 0.3;
/// A looping attack clip goes round its repeated part while the shots come no further
/// apart than this, seconds [engineering choice, as `player_body`'s].
const LOOP_HOLD: f32 = 0.25;

/// An actor starts an attack (from `npc_combat`): a shot or a blow, with the weapon it
/// uses (`None`: fists, or a creature's own attack).
#[derive(Message, Clone, Copy, Debug)]
pub struct PlayAttack {
    pub actor: u32,
    pub weapon: Option<FormId>,
    pub melee: bool,
    /// A shot aimed down the sights: the iron-sights attack clip (`2hrattack3is.kf`).
    pub sights: bool,
}

/// When each actor's next blow lands into its attack clip (its `Hit` key), seconds:
/// `npc_combat` delays the blow's damage by it.
#[derive(Resource, Default)]
pub struct AttackTimes {
    pub hit: HashMap<u32, f32>,
}

/// One attack clip playing.
#[derive(Debug, Clone)]
pub struct Attack {
    pub clip: Arc<BoundClip>,
    pub up: Option<Arc<BoundClip>>,
    pub down: Option<Arc<BoundClip>>,
    pub t: f32,
    pub len: f32,
    /// The repeated part, from the `Fire` key to the `Loop` key (seconds into the clip),
    /// for a clip that has one.
    pub repeat: Option<(f32, f32)>,
    /// Seconds since the last shot or blow.
    pub since: f32,
}

/// A clip's repeated part: from its `Fire` key to its `Loop` key, seconds into it.
fn repeat_of(c: &BoundClip) -> Option<(f32, f32)> {
    let k = ClipKeys::parse(&c.clip.text_keys);
    k.hit.zip(k.loop_end).filter(|(from, to)| from < to).map(|(from, to)| (from - c.clip.start, to - c.clip.start))
}

/// `t` brought back into `from..to` once it passes `to`.
fn loop_time(t: f32, from: f32, to: f32) -> f32 {
    if t < to || from >= to {
        return t;
    }
    from + (t - to).rem_euclid(to - from)
}

/// On a fighting actor's root: its aim's up and down forms, the pitch to its target and
/// the attack playing (read by `locomotion::drive`).
#[derive(Component, Debug, Clone, Default)]
pub struct CombatPose {
    pub aim_up: Option<Arc<BoundClip>>,
    pub aim_down: Option<Arc<BoundClip>>,
    /// The iron-sights aim (`2hraimis.kf`) and its up and down forms, for a weapon that
    /// has them; played in place of the level aim while [`CombatPose::sights`].
    pub aim_is: Option<Arc<BoundClip>>,
    pub aim_is_up: Option<Arc<BoundClip>>,
    pub aim_is_down: Option<Arc<BoundClip>>,
    /// Aiming down the sights now (`npc_combat`).
    pub sights: bool,
    /// Radians, negative looks down.
    pub pitch: f32,
    /// In a fight now (the aim follows the pitch); an attack still playing keeps the
    /// pose after the fight.
    pub fighting: bool,
    pub attack: Option<Attack>,
}

/// The weight to give the up or down form, played beside its level clip at weight 1,
/// so that it has `share` of the blend (the blend divides by the weights' sum).
fn variant_weight(share: f32) -> f32 {
    let s = share.clamp(0.0, MAX_VARIANT);
    s / (1.0 - s)
}

impl CombatPose {
    /// Lay the aim's up or down form and the attack over the actor's layers;
    /// `drawn`: its weapon is drawn (the aim clip plays). Advances the attack by `dt`.
    pub fn layers(&mut self, player: &mut BlendPlayer, drawn: bool, dt: f32) {
        let [_, (_, up), (_, down)] = aim_weights(self.pitch);
        let (variant, share) = if up > 0.0 { (Aim::Up, up) } else { (Aim::Down, down) };
        let w = variant_weight(share);
        // Down the sights: the iron-sights aim in place of the level aim clip (layer 2,
        // `weapons.rs`) while the draw cycle is not fading there.
        let sights = self.fighting && drawn && self.sights && self.aim_is.is_some() && player.layers[AIM_VARIANT_LAYER].is_none();
        if sights {
            if let (Some(level), Some(is)) = (player.layers[2].as_mut(), self.aim_is.clone()) {
                level.clip = is;
            }
        }
        if self.fighting && drawn && player.layers[AIM_VARIANT_LAYER].is_none() && w > 1e-3 {
            let clip = match (variant, sights) {
                (Aim::Up, true) => self.aim_is_up.clone().or(self.aim_up.clone()),
                (Aim::Up, false) => self.aim_up.clone(),
                (_, true) => self.aim_is_down.clone().or(self.aim_down.clone()),
                (_, false) => self.aim_down.clone(),
            };
            player.layers[AIM_VARIANT_LAYER] = clip.map(|clip| Layer { clip, phase: 0.0, weight: w });
        }
        let Some(a) = self.attack.as_mut() else { return };
        a.t += dt;
        a.since += dt;
        if let Some((from, to)) = a.repeat.filter(|_| a.since <= LOOP_HOLD) {
            a.t = loop_time(a.t, from, to);
        }
        if a.t >= a.len {
            self.attack = None;
            return;
        }
        let fade = ((a.t - a.repeat.map_or(0.0, |r| r.0)).max(0.0) / FADE_IN).min(1.0) * ((a.len - a.t) / FADE_OUT).min(1.0);
        let phase = (a.t / a.len.max(1e-3)).min(0.999);
        player.layers[ATTACK_LAYER] = Some(Layer { clip: a.clip.clone(), phase, weight: fade });
        let form = if variant == Aim::Up { a.up.clone() } else { a.down.clone() };
        player.layers[ATTACK_LAYER + 1] = form.filter(|_| w > 1e-3).map(|clip| Layer { clip, phase, weight: w * fade });
    }
}

pub struct AttackAnimsPlugin;

impl Plugin for AttackAnimsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PlayAttack>().init_resource::<AttackTimes>().init_resource::<AttackClips>().add_systems(Update, start).add_systems(Last, report);
    }
}

/// Bound clips by (skeleton, clip path); `None`: not there. Creature attack clips by
/// skeleton folder, and whose turn is next.
#[derive(Resource, Default)]
struct AttackClips {
    bound: HashMap<(String, String), Option<Arc<BoundClip>>>,
    creature: HashMap<String, Vec<String>>,
    turn: HashMap<u32, usize>,
    stats: Stats,
}

#[derive(Default)]
struct Stats {
    started: usize,
    shots: usize,
    blows: usize,
    /// Shots that went on with a looping clip already playing.
    kept: usize,
    missing: BTreeMap<String, usize>,
    clips: BTreeMap<String, usize>,
}

impl AttackClips {
    fn bind(&mut self, ctx: &mut ActorCtx, skeleton: &str, skel: &PreparedSkeleton, path: &str) -> Option<Arc<BoundClip>> {
        let key = (skeleton.to_string(), fnv_vfs::normalize(path));
        self.bound.entry(key).or_insert_with(|| ctx.clip(path).map(|c| Arc::new(bind_clip(skel, c)))).clone()
    }

    /// The first of `paths` (file names in `folder`) that loads.
    fn first(&mut self, ctx: &mut ActorCtx, skeleton: &str, skel: &PreparedSkeleton, folder: &str, names: &[String]) -> Option<(String, Arc<BoundClip>)> {
        names.iter().find_map(|n| {
            let path = format!("{folder}\\{n}");
            self.bind(ctx, skeleton, skel, &path).map(|c| (path, c))
        })
    }
}

/// The folder of a skeleton path (`characters\_male\skeleton.nif` → `meshes\characters\_male`).
fn folder_of(skeleton: &str) -> String {
    let s = fnv_vfs::normalize(&fnv_actor::resolve::mesh_path(skeleton));
    s.rfind('\\').map_or(String::new(), |i| s[..i].to_string())
}

/// The `Hit` text key's time into a clip, if it has one.
fn hit_time(c: &BoundClip) -> Option<f32> {
    c.clip.text_keys.iter().find(|(_, k)| k.trim().eq_ignore_ascii_case("hit")).map(|(t, _)| (t - c.clip.start).max(0.0))
}

type Rigs<'w> = (Entity, &'w ActorRig, Option<&'w WeaponHold>, Option<&'w mut CombatPose>, Has<Dying>);

/// Start the attacks asked for, and keep each fighting actor's pose (module notes).
#[allow(clippy::too_many_arguments)]
fn start(
    mut commands: Commands,
    mut asked: MessageReader<PlayAttack>,
    fighters: Res<Fighters>,
    combat: Option<Res<Combat>>,
    world: Option<Res<ActorWorld>>,
    mut clips: ResMut<AttackClips>,
    mut times: ResMut<AttackTimes>,
    mut parts: ResMut<ActorParts>,
    mut models: ModelCtx,
    mut rigs: Query<Rigs>,
    blocks: Option<ResMut<nvf_play::blocking::ActorBlocks>>,
) {
    // Who is in an attack now: their body blocks hold still (module notes).
    if let Some(mut b) = blocks {
        let now: std::collections::HashSet<u32> = rigs.iter().filter(|r| r.3.is_some_and(|p| p.attack.is_some())).map(|r| r.1.reference).collect();
        if b.attacking != now {
            b.attacking = now;
        }
    }
    let asked: Vec<PlayAttack> = asked.read().copied().collect();
    let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
    let clips = &mut *clips;
    for (e, rig, hold, pose, dying) in &mut rigs {
        let id = rig.reference;
        let pitch = fighters.pitch(id);
        if dying {
            if pose.is_some() {
                commands.entity(e).remove::<CombatPose>();
            }
            continue;
        }
        let mine: Vec<&PlayAttack> = asked.iter().filter(|a| a.actor == id).collect();
        let Some(pitch) = pitch.or(pose.as_ref().filter(|p| p.attack.is_some()).map(|p| p.pitch)) else {
            if pose.is_some() {
                commands.entity(e).remove::<CombatPose>();
            }
            continue;
        };
        let folder = folder_of(&rig.skeleton);
        let skel = rig.skel.clone();
        let weapon_prefix = |w: Option<FormId>| -> &'static str {
            w.and_then(|w| world.as_deref().and_then(|wd| wd.actors.weapons.get(&w))).and_then(|w| w.anim_type.as_deref()).and_then(AnimType::from_name).map_or("h2h", AnimType::prefix)
        };
        let mut new_pose = pose.as_ref().map(|p| (*p).clone()).unwrap_or_default();
        new_pose.pitch = pitch;
        new_pose.fighting = fighters.pitch(id).is_some();
        new_pose.sights = fighters.sights(id);
        // The aim's up and down forms, once, for an actor with a weapon drawn on it.
        if hold.is_some() && new_pose.aim_up.is_none() && new_pose.aim_down.is_none() {
            let w = fighters.weapon(id);
            let level = format!("{folder}\\{}aim.kf", weapon_prefix(w));
            new_pose.aim_up = clips.bind(&mut ctx, &rig.skeleton, &skel, &aim_variant(&level, Aim::Up));
            new_pose.aim_down = clips.bind(&mut ctx, &rig.skeleton, &skel, &aim_variant(&level, Aim::Down));
            // The iron-sights aim, where the stance has one (rifles, pistols, automatics).
            let is = format!("{folder}\\{}aimis.kf", weapon_prefix(w));
            new_pose.aim_is = clips.bind(&mut ctx, &rig.skeleton, &skel, &is);
            if new_pose.aim_is.is_some() {
                new_pose.aim_is_up = clips.bind(&mut ctx, &rig.skeleton, &skel, &aim_variant(&is, Aim::Up));
                new_pose.aim_is_down = clips.bind(&mut ctx, &rig.skeleton, &skel, &aim_variant(&is, Aim::Down));
            }
        }
        for a in mine {
            let creature = hold.is_none() && a.weapon.is_none() && rig.skeleton.to_ascii_lowercase().contains("creatures");
            let found = if creature {
                // The creature's own attack clips, in turn.
                let list = clips
                    .creature
                    .entry(folder.clone())
                    .or_insert_with(|| {
                        let pre = format!("{folder}\\h2hattack");
                        ctx.models
                            .data
                            .vfs
                            .paths(|p| p.starts_with(&pre) && p.ends_with(".kf") && !p[folder.len() + 1..].contains('\\') && !p.contains("power") && !p.contains("forward"))
                            .into_iter()
                            .map(|p| p[folder.len() + 1..].to_string())
                            .collect()
                    })
                    .clone();
                let k = clips.turn.entry(id).or_insert(0);
                let n = list.len();
                let names: Vec<String> = (0..n).map(|i| list[(*k + i) % n.max(1)].clone()).collect();
                *k = (*k + 1) % n.max(1);
                clips.first(&mut ctx, &rig.skeleton, &skel, &folder, &names)
            } else {
                let prefix = weapon_prefix(a.weapon);
                let attack = a.weapon.and_then(|w| combat.as_deref().and_then(|c| c.forms.weapons.get(&w))).and_then(|w| w.attack_anim.clone());
                let names = attack_clips(Stance::Stand, prefix, &attack_stem(attack.as_deref()), a.sights);
                clips.first(&mut ctx, &rig.skeleton, &skel, &folder, &names)
            };
            let Some((path, clip)) = found else {
                *clips.stats.missing.entry(format!("{folder} {}", if creature { "creature attack".to_string() } else { weapon_prefix(a.weapon).to_string() })).or_default() += 1;
                continue;
            };
            // A looping clip still running goes on through this shot (module notes).
            if let Some(cur) = new_pose.attack.as_mut().filter(|c| c.repeat.is_some() && Arc::ptr_eq(&c.clip, &clip) && c.t < c.len) {
                cur.since = 0.0;
                clips.stats.kept += 1;
                continue;
            }
            let up = clips.bind(&mut ctx, &rig.skeleton, &skel, &aim_variant(&path, Aim::Up));
            let down = clips.bind(&mut ctx, &rig.skeleton, &skel, &aim_variant(&path, Aim::Down));
            let len = clip.clip.length().max(0.05);
            if a.melee {
                times.hit.insert(id, hit_time(&clip).unwrap_or(DEFAULT_HIT).min(len));
            }
            let st = &mut clips.stats;
            st.started += 1;
            if a.melee {
                st.blows += 1;
            } else {
                st.shots += 1;
            }
            let name = path.rsplit('\\').next().unwrap_or(&path).to_string();
            if !st.clips.contains_key(&name) {
                info!("attack anims: {id:08X} plays {path} ({len:.2} s{})", hit_time(&clip).map_or(String::new(), |h| format!(", hit at {h:.2} s")));
            }
            *st.clips.entry(name).or_default() += 1;
            let repeat = repeat_of(&clip).filter(|_| !a.melee);
            let t = repeat.map_or(0.0, |(from, _)| from);
            new_pose.attack = Some(Attack { clip, up, down, t, len, repeat, since: 0.0 });
        }
        match pose {
            Some(mut p) => *p = new_pose,
            None => {
                commands.entity(e).insert(new_pose);
            }
        }
    }
}

/// Whether a weapon hold has its weapon drawn (the aim clip playing).
pub fn drawn(hold: Option<&WeaponHold>) -> bool {
    hold.is_some_and(|h| h.anim.stage == Stage::Drawn)
}

fn report(mut exit: MessageReader<AppExit>, clips: Res<AttackClips>) {
    if exit.read().next().is_none() {
        return;
    }
    let s = &clips.stats;
    let list = |m: &BTreeMap<String, usize>| if m.is_empty() { "none".to_string() } else { m.iter().map(|(k, n)| format!("{k} {n}")).collect::<Vec<_>>().join(", ") };
    info!(
        "attack anims summary: {} attack clips started ({} shots, {} blows), {} shots went on with a looping clip: {}; without a clip: {}",
        s.started,
        s.shots,
        s.blows,
        s.kept,
        list(&s.clips),
        list(&s.missing)
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_variant_weight_gives_its_share_beside_the_level_clip() {
        for share in [0.0f32, 0.25, 0.5, 0.9] {
            let w = variant_weight(share);
            assert!((w / (1.0 + w) - share).abs() < 1e-5, "{share}");
        }
        assert!(variant_weight(1.0).is_finite());
    }

    #[test]
    fn a_looping_clip_goes_round_from_fire_to_loop() {
        // The install's 2hhattackspin.kf: `Fire` at 0.2 s, `Loop` at 0.867 s.
        assert_eq!(loop_time(0.5, 0.2, 0.867), 0.5);
        assert!((loop_time(0.9, 0.2, 0.867) - 0.233).abs() < 1e-5);
        assert_eq!(loop_time(0.9, 0.9, 0.2), 0.9);
    }

    #[test]
    fn a_skeletons_folder() {
        assert_eq!(folder_of(r"Creatures\Dog\Skeleton.NIF"), r"meshes\creatures\dog");
        assert_eq!(folder_of(r"meshes\characters\_male\skeleton.nif"), r"meshes\characters\_male");
    }
}
