//! Animation groups scripts play on placed objects: `PlayGroup` (fnv-game `anims`;
//! docs/spec-notes/activators.md §5).
//!
//! - **Which objects.** Every drawn object whose model has a named sequence that animates
//!   it gets a [`GroupRig`] (world.rs): those that play an idle or open like a door, and
//!   those with a sequence only scripts play (Forward, Backward, Left, Right, …), which
//!   are drawn with their own node hierarchy for this [design: any reference can be
//!   named by a script, `Ref.PlayGroup`, so the rig does not wait for the first call;
//!   the cost is a node hierarchy per such reference].
//! - **A request** names a group; the sequence of the same name is played
//!   [hypothesis: the names match, activators.md §5]. Its cycle type decides how: a
//!   clamped sequence plays once and holds its end, a looping one loops until another
//!   group replaces it [hypothesis: as the NIF cycle types]. The start flag
//!   [documented: GECK wiki PlayGroup]: 1 and 2 start at once from the beginning (no
//!   activator sequence marks a loop start, so 2 is as 1 [observed: activators.md §5]);
//!   0 lets what plays finish: a group played once until its end, a loop (the object's
//!   own idle included) until its cycle ends. A loop asked for again while it plays
//!   goes on [design].
//! - **What the rules see**: the engine tells `fnv_game` (`ObjectAnims::set_playing`)
//!   when a group cannot be played (no such sequence, the object not drawn) and when one
//!   played once ends; `IsAnimPlaying` answers from that.
//! - **Not drawn**: the request is kept and shown when the object is drawn: a group
//!   played once at its end, a loop from its start [design].
//! - **Sounds**: the `Sound: <editor ID>` text keys the playing group passes are played
//!   where the object is (anim_sounds.rs, [`ObjectSound`]).
//!
//! A node track on the rig's root is not applied: the root carries the placement
//! [design]. Log: `playgroup: <ref> plays <group> (once|loop, <s> s, flag n)`, and at
//! exit `playgroup summary: …`.

use bevy::prelude::*;
use fnv_game::anims::{AnimRequest, Start};
use fnv_sim::FormId;
use nvf_assets::anim_play::{Clip, Playing};
use nvf_assets::material_anim::RigMaterials;
use nvf_assets::model::PreparedModel;
use nvf_play::script::ScriptState;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// NIF cycle type "clamp": played once [documented: nif.xml CycleType].
const CLAMP: i64 = 2;

/// On the root of a drawn object whose model has named sequences.
#[derive(Component)]
pub struct GroupRig {
    pub form_id: u32,
    model: Arc<PreparedModel>,
    source: String,
    nodes: HashMap<String, Entity>,
    /// The group playing: its code and the index of its clip in the rig's groups.
    current: Option<(u16, usize)>,
    /// A group waiting for what plays to finish (flag 0).
    next: Option<(u16, usize)>,
    /// Sequence time and cycle count last seen, to find passed keys and cycle ends.
    last: Option<(f32, i64)>,
}

impl GroupRig {
    pub fn new(form_id: u32, model: Arc<PreparedModel>, source: &str, nodes: HashMap<String, Entity>) -> GroupRig {
        GroupRig { form_id, model, source: source.to_string(), nodes, current: None, next: None, last: None }
    }

    fn clip(&self, i: usize) -> Option<&Clip> {
        self.model.rig.as_ref()?.groups.get(i)
    }

    fn find(&self, name: &str) -> Option<usize> {
        self.model.rig.as_ref()?.groups.iter().position(|c| c.name.eq_ignore_ascii_case(name))
    }
}

/// A sound key a playing group passed: SOUN editor ID, where (game units).
#[derive(Message, Clone, Debug)]
pub struct ObjectSound {
    pub at: [f32; 3],
    pub key: String,
}

#[derive(Resource, Default)]
pub struct ObjectAnims {
    /// Requests for objects not drawn, shown when they are.
    waiting: HashMap<u32, AnimRequest>,
    /// The last request logged per reference (a script repeating it is not logged again).
    logged: HashMap<u32, (u16, i32)>,
    stats: Stats,
}

#[derive(Default)]
struct Stats {
    asked: u32,
    started: u32,
    queued: u32,
    already: u32,
    ended: u32,
    not_drawn: u32,
    shown_when_drawn: u32,
    no_sequence: u32,
    sounds: u32,
    by_group: BTreeMap<String, u32>,
}

/// Start clip `i` of `rig` on its root (replacing what played there).
fn start(commands: &mut Commands, root: Entity, rig: &mut GroupRig, group: u16, i: usize, at_end: bool, mats: Option<&mut RigMaterials>) {
    let Some(clip) = rig.clip(i) else { return };
    let root_name = rig.model.rig.as_ref().and_then(|r| r.nodes.first()).and_then(|n| n.name.clone());
    let mut p = Playing::new(&rig.source, clip, |n| if Some(n) == root_name.as_deref() { None } else { rig.nodes.get(n).copied() });
    if at_end {
        p.time = clip.stop;
    }
    if let Some(m) = mats {
        m.idle = Arc::new(clip.materials.clone());
        m.dark = false;
    }
    commands.entity(root).insert(p);
    rig.current = Some((group, i));
    rig.next = None;
    rig.last = None;
}

fn describe(clip: &Clip) -> String {
    format!("{} ({}, {:.2} s)", clip.name, if clip.cycle_type == CLAMP { "once" } else { "loop" }, clip.length())
}

/// Take the game's requests and play them; follow each playing group's end, its queued
/// successor and its sound keys.
#[allow(clippy::type_complexity)]
pub fn drive(
    mut commands: Commands,
    mut st: ResMut<ObjectAnims>,
    mut script: Option<ResMut<ScriptState>>,
    mut rigs: Query<(Entity, &mut GroupRig, Option<&Playing>, Option<&mut RigMaterials>, &GlobalTransform)>,
    mut sounds: MessageWriter<ObjectSound>,
) {
    let st = &mut *st;
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let game = &mut rt.game;
    game.anims.driven = true;
    let requests = std::mem::take(&mut game.anims.requests);
    let by_ref: HashMap<u32, Entity> = rigs.iter().map(|(e, r, ..)| (r.form_id, e)).collect();
    // Rigs started this frame: their new clip is on them from the next frame.
    let mut started: std::collections::HashSet<Entity> = std::collections::HashSet::new();

    // Objects drawn since last frame that a script asked for while they were not.
    for (root, mut rig, _, mats, _) in &mut rigs {
        if !rig.is_added() || st.waiting.is_empty() {
            continue;
        }
        let Some(r) = st.waiting.remove(&rig.form_id) else { continue };
        let Some(name) = r.name.as_deref() else { continue };
        let Some(i) = rig.find(name) else { continue };
        let once = rig.clip(i).is_some_and(|c| c.cycle_type == CLAMP);
        start(&mut commands, root, &mut rig, r.group, i, once, mats.map(|m| m.into_inner()));
        started.insert(root);
        st.stats.shown_when_drawn += 1;
        if once {
            game.anims.set_playing(r.on, None);
        }
        info!("playgroup: {} drawn, shows {name} as asked while it was not", game.data.name(r.on));
    }

    for r in requests {
        st.stats.asked += 1;
        let who = game.data.name(r.on);
        let group = r.name.clone().unwrap_or_else(|| format!("code {}", r.group));
        *st.stats.by_group.entry(group.clone()).or_default() += 1;
        let fresh = st.logged.insert(r.on.0, (r.group, r.start.flag())) != Some((r.group, r.start.flag()));
        let Some(&e) = by_ref.get(&r.on.0) else {
            st.stats.not_drawn += 1;
            if fresh {
                info!("playgroup: {who} plays {group} (flag {}): not drawn, shown when it is", r.start.flag());
            }
            game.anims.set_playing(r.on, None);
            st.waiting.insert(r.on.0, r);
            continue;
        };
        let Ok((root, mut rig, playing, mats, _)) = rigs.get_mut(e) else { continue };
        let Some(i) = r.name.as_deref().and_then(|n| rig.find(n)) else {
            st.stats.no_sequence += 1;
            if fresh {
                info!("playgroup: {who} plays {group}: its model has no sequence of that name");
            }
            game.anims.set_playing(r.on, None);
            continue;
        };
        let desc = rig.clip(i).map(describe).unwrap_or_default();
        // The loop asked for already plays (a script asking every frame until it sees
        // it play): left as it is [design].
        let looping = rig.clip(i).is_some_and(|c| c.cycle_type != CLAMP);
        if looping && (rig.current == Some((r.group, i)) || rig.next == Some((r.group, i))) {
            st.stats.already += 1;
            continue;
        }
        // Flag 0: what plays finishes first (a group played once that has not ended, or
        // any loop, the idle included).
        let busy = playing.is_some_and(|p| !(p.cycle_type == CLAMP && p.time >= p.stop) && !p.held);
        if r.start == Start::Normal && busy {
            rig.next = Some((r.group, i));
            st.stats.queued += 1;
            if fresh {
                info!("playgroup: {who} plays {desc}, flag 0: after {} ends its cycle", playing.map(|p| p.name.as_str()).unwrap_or("-"));
            }
            continue;
        }
        start(&mut commands, root, &mut rig, r.group, i, false, mats.map(|m| m.into_inner()));
        started.insert(root);
        st.stats.started += 1;
        if fresh {
            info!("playgroup: {who} plays {desc}, flag {}", r.start.flag());
        }
    }

    // Ends, queued groups and sound keys.
    for (root, mut rig, playing, mats, at) in &mut rigs {
        let Some(p) = playing.filter(|_| !started.contains(&root)) else { continue };
        let len = (p.stop - p.start).max(1e-6);
        let cycles = ((p.time - p.start) / len).floor() as i64;
        let t = p.sequence_time();
        let ended = p.cycle_type == CLAMP && p.time >= p.stop;
        let wrapped = rig.last.is_some_and(|(_, c)| c != cycles);
        // Sound keys of the group playing, passed since last frame.
        if let (Some((_, i)), Some((t0, c0))) = (rig.current, rig.last) {
            if let Some(clip) = rig.clip(i) {
                let from = if c0 != cycles { p.start - 1.0 } else { t0 };
                for (kt, k) in &clip.text_keys {
                    if *kt > from && *kt <= t {
                        if let Some(key) = fnv_collide::anim::AnimSequence::sound_of(k) {
                            st.stats.sounds += 1;
                            let w = crate::world::bevy_to_game(at.translation()).to_array();
                            sounds.write(ObjectSound { at: w, key: key.to_string() });
                        }
                    }
                }
            }
        }
        rig.last = Some((t, cycles));
        if let Some((g, i)) = rig.next {
            if ended || wrapped {
                start(&mut commands, root, &mut rig, g, i, false, mats.map(|m| m.into_inner()));
                st.stats.started += 1;
                if let Some(c) = rig.clip(i) {
                    info!("playgroup: {} plays {}, its turn come", game.data.name(FormId(rig.form_id)), describe(c));
                }
                continue;
            }
        }
        if ended {
            if let Some((g, _)) = rig.current.take() {
                st.stats.ended += 1;
                let id = FormId(rig.form_id);
                if game.anims.playing(id) == Some(g) {
                    game.anims.set_playing(id, None);
                }
                debug!("playgroup: {} holds the end of its group", game.data.name(id));
            }
        }
    }
}

/// `playgroup summary: …` at exit.
pub fn report(mut exit: MessageReader<AppExit>, st: Res<ObjectAnims>) {
    if exit.read().last().is_none() {
        return;
    }
    let s = &st.stats;
    let groups: Vec<String> = s.by_group.iter().map(|(g, n)| format!("{g} {n}")).collect();
    info!(
        "playgroup summary: {} asked ({}); {} started, {} waited for the playing one, {} loops already playing, {} played once and ended, {} for objects not drawn ({} shown when drawn), {} without a sequence of that name; {} sound keys",
        s.asked,
        if groups.is_empty() { "-".to_string() } else { groups.join(", ") },
        s.started,
        s.queued,
        s.already,
        s.ended,
        s.not_drawn,
        s.shown_when_drawn,
        s.no_sequence,
        s.sounds
    );
}

/// Added by nvforever with `--play`.
pub struct ObjectAnimsPlugin;

impl Plugin for ObjectAnimsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ObjectAnims>()
            .add_message::<ObjectSound>()
            .add_systems(Update, drive.after(nvf_play::PlaySet::Scripts).before(crate::advance_animations))
            .add_systems(Last, report);
    }
}
