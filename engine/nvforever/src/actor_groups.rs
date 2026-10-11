//! Animation groups scripts play on actors: `PlayGroup` on an NPC or a creature (fnv-game
//! `anims`; docs/spec-notes/activators.md §7, idles.md §7).
//!
//! - **Which clip.** The group names a clip of the actor's own animation set, found
//!   [observed: activators.md §7.2, the sequence names of the base archives' `.kf`
//!   files by skeleton folder] as, in this order: the clip of that name of the weapon
//!   stance the actor holds (`<prefix>aim.kf`, `<prefix>equip.kf`, …); the movement clip of
//!   that name of its locomotion set (Forward, FastLeft, TurnLeft, …); else a file of the
//!   skeleton's folder: `h2h<group>.kf` (the unarmed stance: 27 of the 35 folders with
//!   an Aim clip), `specialanims\h2h<group>.kf`, `<group>.kf`,
//!   `mt<group>.kf`, `locomotion\mt<group>.kf`. A file counts only if its sequence has
//!   the group's name. SpecialIdle names no one clip (its clips are the IDLE records'
//!   `SpecialIdle_*`, picked by the idle manager), so it plays nothing here [design].
//! - **Idle** returns the actor to normal [documented: GECK wiki PlayGroup]: the group
//!   (or idle) it plays fades out and its own animation shows again.
//! - **Playing.** The clip plays in the actor's idle slot (`idles::ActorIdle`, the top
//!   blend layers) as an idle does, fading in and out by its text keys
//!   (`fnv_actor::idle_play`): a clamped clip once, a looping one until another group or
//!   idle replaces it, `PlayGroup Idle`, or the actor walks [design: the actor's own
//!   animation takes over when it moves]. Flag 0 waits for what plays in the slot (a
//!   loop leaves its loop at the end of its pass); flags 1 and 2 start at once
//!   [documented flag meanings, as for objects; flag 2 is taken as 1, design].
//! - **What the rules see**: a group counts as playing from the call on
//!   (`ObjectAnims::driven`) until its clip ends or is replaced, or it cannot be played
//!   (no clip, the actor not drawn); `IsAnimPlaying` answers from that. An actor's idles
//!   do not count as groups, and a group is not an idle for `IsIdlePlaying`.
//!
//! Log: `actor group: <actor> plays <group> from the <where>, once|loop, <s> s, flag <n>`,
//! and at exit `actor group summary: …`.

use crate::idles::ActorIdle;
use crate::limbs::ActorRig;
use crate::locomotion::Locomotion;
use crate::weapons::WeaponHold;
use bevy::prelude::*;
use fnv_actor::idle_play::{IdleKeys, IdlePlay};
use fnv_actor::locomotion::Move;
use fnv_game::anims::{AnimRequest, Start};
use fnv_sim::FormId;
use nvf_assets::actor::{bind_clip, ActorCtx, ActorParts};
use nvf_assets::anim_play::BoundClip;
use nvf_assets::model::ModelCtx;
use nvf_play::script::ScriptState;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// NIF cycle type "clamp": played once [documented: nif.xml CycleType].
const CLAMP: i64 = 2;

/// The stance prefix of an actor without a weapon [observed: the unarmed `h2h` clips,
/// animation.md §3.2].
const UNARMED: &str = "h2h";

/// The files of a skeleton folder that may hold `group`'s clip, in the order tried
/// (module docs).
pub fn file_candidates(folder: &str, group: &str) -> Vec<String> {
    let g = group.to_ascii_lowercase();
    vec![
        format!("{folder}\\{UNARMED}{g}.kf"),
        format!("{folder}\\specialanims\\{UNARMED}{g}.kf"),
        format!("{folder}\\{g}.kf"),
        format!("{folder}\\mt{g}.kf"),
        format!("{folder}\\locomotion\\mt{g}.kf"),
    ]
}

/// What happened to a request, for the summary.
#[derive(Default)]
struct Stats {
    asked: u32,
    started: u32,
    already: u32,
    queued: u32,
    to_normal: u32,
    ended: u32,
    not_drawn: u32,
    no_clip: u32,
    by_group: BTreeMap<String, u32>,
    by_source: BTreeMap<&'static str, u32>,
}

#[derive(Resource, Default)]
pub struct ActorGroups {
    /// Requests for actors, handed over by `object_anims::drive`.
    pub queue: Vec<AnimRequest>,
    /// Flag-0 requests waiting for what plays in the actor's slot.
    waiting: HashMap<u32, (AnimRequest, Arc<BoundClip>)>,
    /// The group each actor was last started on, by the serial of its slot entry.
    told: HashMap<u32, (u16, u32)>,
    /// Clips found, by skeleton, stance prefix and group (`None`: none found).
    found: HashMap<(String, String, u16), Option<(Arc<BoundClip>, &'static str)>>,
    /// The last request logged per actor (a script repeating it is not logged again).
    logged: HashMap<u32, (u16, i32)>,
    stats: Stats,
}

/// The clip of `group` for an actor (module docs), with where it came from.
fn find_clip(
    ctx: &mut ActorCtx,
    rig: &ActorRig,
    weapon: Option<&WeaponHold>,
    lo: Option<&Locomotion>,
    group: &str,
) -> Option<(Arc<BoundClip>, &'static str)> {
    if let Some(c) = weapon.and_then(|w| w.stance.clips.values().find(|c| c.clip.name.eq_ignore_ascii_case(group))) {
        return Some((c.clone(), "weapon stance"));
    }
    if let Some(c) = Move::ALL.iter().find(|m| m.group().eq_ignore_ascii_case(group)).and_then(|m| lo?.set.clips.get(m)) {
        return Some((c.bound.clone(), "locomotion set"));
    }
    let s = fnv_vfs::normalize(&rig.skeleton);
    let folder = s.rfind('\\').map_or("", |i| &s[..i]).to_string();
    for path in file_candidates(&folder, group) {
        if !ctx.models.data.vfs.exists(&path) {
            continue;
        }
        let Some(clip) = ctx.clip(&path).filter(|c| c.name.eq_ignore_ascii_case(group)) else { continue };
        let bound = bind_clip(&rig.skel, clip);
        if !bound.tracks.is_empty() {
            return Some((Arc::new(bound), "skeleton folder"));
        }
    }
    None
}

fn play_of(clip: &BoundClip) -> IdlePlay {
    let c = &clip.clip;
    let keys = IdleKeys::read(c.start, c.stop, &c.text_keys);
    IdlePlay::new(keys, (c.cycle_type == CLAMP).then_some(1))
}

fn describe(clip: &BoundClip, from: &str) -> String {
    let c = &clip.clip;
    format!("{} from the {from}, {}, {:.2} s", c.name, if c.cycle_type == CLAMP { "once" } else { "loop" }, c.length())
}

/// What [`drive`] reads and writes of a drawn actor.
type Drawn = (
    Entity,
    &'static ActorRig,
    Option<&'static mut ActorIdle>,
    Option<&'static Locomotion>,
    Option<&'static WeaponHold>,
);

/// Take the actors' requests and play them; tell the rules when a group ends.
#[allow(clippy::too_many_arguments)]
pub fn drive(
    mut commands: Commands,
    mut st: ResMut<ActorGroups>,
    mut script: Option<ResMut<ScriptState>>,
    parts: Option<ResMut<ActorParts>>,
    mut models: ModelCtx,
    mut actors: Query<Drawn>,
) {
    let st = &mut *st;
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else {
        st.queue.clear();
        return;
    };
    let game = &mut rt.game;
    let by_ref: HashMap<u32, Entity> = actors.iter().map(|(e, r, ..)| (r.reference, e)).collect();

    // Groups that ended, were replaced, or that the actor walked out of; flag-0 requests
    // whose turn came.
    let told: Vec<(u32, (u16, u32))> = st.told.iter().map(|(a, t)| (*a, *t)).collect();
    for (actor, (group, serial)) in told {
        let slot = by_ref.get(&actor).and_then(|e| actors.get_mut(*e).ok());
        let Some((_, _, Some(mut idle), lo, _)) = slot else {
            st.told.remove(&actor);
            if game.anims.playing(FormId(actor)) == Some(group) {
                game.anims.set_playing(FormId(actor), None);
            }
            continue;
        };
        let walking = lo.is_some_and(|l| l.velocity != [0.0; 2]);
        if let Some(c) = idle.0.cur.as_mut().filter(|c| c.serial == serial && c.group == Some(group) && walking && c.play.endless()) {
            c.play.stop();
        }
        let ours = idle.0.playing().is_some_and(|c| c.serial == serial && c.group == Some(group));
        if !ours {
            st.told.remove(&actor);
            st.stats.ended += 1;
            if game.anims.playing(FormId(actor)) == Some(group) {
                game.anims.set_playing(FormId(actor), None);
            }
            debug!("actor group: {} ends {}", game.data.name(FormId(actor)), game.anim_group_name(group).unwrap_or_default());
        }
    }
    let waiting: Vec<u32> = st.waiting.keys().copied().collect();
    for actor in waiting {
        let Some(&e) = by_ref.get(&actor) else {
            if let Some((r, _)) = st.waiting.remove(&actor) {
                game.anims.set_playing(r.on, None);
            }
            continue;
        };
        let Ok((_, _, idle, ..)) = actors.get_mut(e) else { continue };
        if idle.as_ref().is_some_and(|i| i.0.playing().is_some()) {
            continue;
        }
        let Some((r, clip)) = st.waiting.remove(&actor) else { continue };
        let serial = start(&mut commands, e, idle.map(|i| i.into_inner()), r.group, clip.clone());
        st.told.insert(actor, (r.group, serial));
        st.stats.started += 1;
        info!("actor group: {} plays {}, its turn come", game.data.name(r.on), describe(&clip, "queue"));
    }

    // New requests.
    let requests = std::mem::take(&mut st.queue);
    if requests.is_empty() {
        return;
    }
    let Some(mut parts) = parts else {
        for r in requests {
            game.anims.set_playing(r.on, None);
        }
        return;
    };
    let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
    for r in requests {
        let who = game.data.name(r.on);
        let group = r.name.clone().unwrap_or_else(|| format!("code {}", r.group));
        st.stats.asked += 1;
        *st.stats.by_group.entry(group.clone()).or_default() += 1;
        let fresh = st.logged.insert(r.on.0, (r.group, r.start.flag())) != Some((r.group, r.start.flag()));
        let Some(&e) = by_ref.get(&r.on.0) else {
            st.stats.not_drawn += 1;
            if fresh {
                info!("actor group: {who} plays {group} (flag {}): the actor is not drawn", r.start.flag());
            }
            game.anims.set_playing(r.on, None);
            continue;
        };
        let Ok((_, rig, idle, lo, weapon)) = actors.get_mut(e) else { continue };
        if group.eq_ignore_ascii_case("Idle") {
            // Back to normal: what plays in the slot fades out.
            if let Some(mut i) = idle {
                if let Some(c) = i.0.cur.as_mut() {
                    c.play.stop();
                }
            }
            st.waiting.remove(&r.on.0);
            st.told.remove(&r.on.0);
            st.stats.to_normal += 1;
            game.anims.set_playing(r.on, None);
            if fresh {
                info!("actor group: {who} plays Idle (flag {}): back to its own animation", r.start.flag());
            }
            continue;
        }
        let prefix = weapon.map_or(UNARMED.to_string(), |w| w.stance.prefix.clone());
        let key = (rig.skeleton.clone(), prefix, r.group);
        let found = match st.found.get(&key) {
            Some(f) => f.clone(),
            None => {
                let f = find_clip(&mut ctx, rig, weapon, lo, &group);
                st.found.insert(key, f.clone());
                f
            }
        };
        let Some((clip, from)) = found else {
            st.stats.no_clip += 1;
            if fresh {
                info!("actor group: {who} plays {group}: no clip of that name for its skeleton {}", rig.skeleton);
            }
            game.anims.set_playing(r.on, None);
            continue;
        };
        *st.stats.by_source.entry(from).or_default() += 1;
        // The loop asked for already plays (a script asking every frame until it sees it
        // play): left as it is [design, as for objects].
        let playing_it = st.told.get(&r.on.0).is_some_and(|(g, serial)| {
            *g == r.group && idle.as_ref().is_some_and(|i| i.0.playing().is_some_and(|c| c.serial == *serial && c.play.endless()))
        });
        if playing_it {
            st.stats.already += 1;
            continue;
        }
        let busy = idle.as_ref().is_some_and(|i| i.0.playing().is_some());
        if r.start == Start::Normal && busy {
            if let Some(mut i) = idle {
                if let Some(c) = i.0.cur.as_mut().filter(|c| c.play.endless()) {
                    c.play.leave_loop();
                }
            }
            if fresh {
                info!("actor group: {who} plays {}, flag 0: after what it plays ends", describe(&clip, from));
            }
            st.waiting.insert(r.on.0, (r, clip));
            st.stats.queued += 1;
            continue;
        }
        let serial = start(&mut commands, e, idle.map(|i| i.into_inner()), r.group, clip.clone());
        st.waiting.remove(&r.on.0);
        st.told.insert(r.on.0, (r.group, serial));
        game.anims.set_playing(r.on, Some(r.group));
        st.stats.started += 1;
        if fresh {
            info!("actor group: {who} plays {}, flag {}", describe(&clip, from), r.start.flag());
        }
    }
}

/// Start `clip` in the actor's idle slot (made if it has none); the slot entry's serial.
fn start(commands: &mut Commands, e: Entity, idle: Option<&mut ActorIdle>, group: u16, clip: Arc<BoundClip>) -> u32 {
    let play = play_of(&clip);
    match idle {
        Some(i) => i.0.start_group(group, clip, play),
        None => {
            let mut i = ActorIdle::default();
            let serial = i.0.start_group(group, clip, play);
            commands.entity(e).insert(i);
            serial
        }
    }
}

/// `actor group summary: …` at exit.
pub fn report(mut exit: MessageReader<AppExit>, st: Res<ActorGroups>) {
    if exit.read().last().is_none() {
        return;
    }
    let s = &st.stats;
    let list = |m: &BTreeMap<String, u32>| if m.is_empty() { "-".to_string() } else { m.iter().map(|(g, n)| format!("{g} {n}")).collect::<Vec<_>>().join(", ") };
    let from = if s.by_source.is_empty() { "-".to_string() } else { s.by_source.iter().map(|(g, n)| format!("{g} {n}")).collect::<Vec<_>>().join(", ") };
    info!(
        "actor group summary: {} asked ({}); {} started (clips from: {from}), {} loops already playing, {} waited for what played, {} back to normal by Idle, {} ended or replaced, {} for actors not drawn, {} without a clip",
        s.asked,
        list(&s.by_group),
        s.started,
        s.already,
        s.queued,
        s.to_normal,
        s.ended,
        s.not_drawn,
        s.no_clip
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_tried_for_a_group() {
        let c = file_candidates("meshes\\creatures\\dog", "Aim");
        assert_eq!(c[0], "meshes\\creatures\\dog\\h2haim.kf");
        assert_eq!(c[1], "meshes\\creatures\\dog\\specialanims\\h2haim.kf");
        assert_eq!(c[3], "meshes\\creatures\\dog\\mtaim.kf");
    }
}
