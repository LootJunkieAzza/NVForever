//! Co-op replication, the test build's engine side (docs/design/coop-replication.md):
//! the host's world is the one world, and each guest's game draws it.
//!
//! - **Change log (§6.1, phase 2).** The host's `fnv_sim::World` keeps the references
//!   written to (`World::set_tracking`, `take_touched`). Every [`TICK`] the host reads
//!   the components of the touched ones (and every [`SWEEP`] of all it holds near each
//!   guest, so a reference coming into a guest's area goes too), and sends each guest
//!   what differs from what it last sent that guest ([`nvf_coop::repl::Shadow`]), on the
//!   reliable stream. A value still as the plugins place it is not sent: both games
//!   have it.
//! - **Desync detector.** Every [`DIGEST_EVERY`] a guest hashes what it holds of the
//!   references it has made in its area; the host compares with its own, logs each one
//!   that differs (`replication desync:`) and sends it whole again. That also takes
//!   back what the guest's own save had changed there.
//! - **Looks (phase 1).** For every actor the host draws: its base, the level and rolls of
//!   its leveled picks, and the items it wears and holds (`World::wear_as` on the guest).
//!   Worn items come from the actor's inventory, rolled with the world's sequential
//!   random numbers, so two games disagree on them (queue items 28, 71); they are sent.
//! - **Actors (phase 3).** The moving actors go on the compact stream, 18 bytes each,
//!   at 20, 10 or 3 Hz by distance and view, only when their pose changed (and once a
//!   second while they stand), in place of the old `Npcs` message.
//! - **Loot stays per-player (owner, 2026-11-07).** Container and corpse contents are not
//!   sent, and a loose item one player takes stays for the others (its `deleted` flag is
//!   not replicated nor compared).
//! - **Players' drops are shared (owner, 2026-11-09, queue item 135).** An item a player
//!   drops (`RefState::player_drop`) has one copy, in the host's world: every player sees
//!   it ([`Value::Item`]) and its `deleted` flag goes to all, so whoever takes it takes it
//!   for everyone. A guest's drop goes to the host once it comes to rest
//!   ([`Loose::Dropped`]); the host makes it and tells the guest its number
//!   ([`Loose::Made`]), and the guest's own copy takes that number. A guest's pickup is
//!   its own game's at once and a request to the host ([`Loose::Took`]); the host's world
//!   decides who was first, and a guest that was not gives the item back
//!   ([`Loose::Refused`]), so nothing is doubled. The host's save keeps them.
//! - **Perf (item 72).** Every [`PERF_EVERY`] a `replication perf:` line on each side.
//!
//! Solo: every system returns at once without a lobby (R1).

use super::*;
use nvf_coop::repl::{self, comp, ActorPose, Change, Digest, Effect, Loose, Shadow, Value};
use std::collections::BTreeSet;
use std::time::Instant;
use bevy::text::FontSize;

/// How often the host sends changes and actor poses.
const TICK: f32 = 0.05;
/// How often the host goes over every reference it holds near each guest.
const SWEEP: f32 = 1.0;
/// How often a guest sends its digest.
const DIGEST_EVERY: f32 = 3.0;
/// A reference changed this recently is in flight, not out of step (`Repl::sent_at`)
/// [engineering choice: two digests].
const SETTLE: f32 = 2.0 * DIGEST_EVERY;
/// How often each side writes its perf line.
const PERF_EVERY: f32 = 5.0;
/// A guest's area: its interior cell, or this far around it outdoors (game units). The
/// leash keeps it within 6000 of the host.
const REACH: f32 = 9000.0;
/// A standing actor's pose goes again this often, so the guest keeps it.
const KEEP_ALIVE: f32 = 1.0;
/// How often a guest sends the pose of what it carries (seconds).
const CARRY_EVERY: f32 = 0.05;
/// A dropped or thrown object is followed at most this long before its last pose goes.
const CARRY_FALL_MAX: f32 = 10.0;
/// The most a reliable item takes (under `rel::ITEM_MAX`).
const ITEM: usize = 1000;
/// Env var: the host's numbers in the window's corner.
pub const HUD_ENV: &str = "NVF_REPLICATION_HUD";
/// Env var (a test aid): the perf line also without a lobby, the solo baseline the
/// host's numbers are compared with. Without it solo runs none of this module.
pub const PERF_ENV: &str = "NVF_REPLICATION_PERF";

fn solo_perf() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var(PERF_ENV).is_ok_and(|v| v == "1"))
}

#[derive(Default)]
pub(super) struct Repl {
    /// A guest's: where the host's script moved the host's player (interior cell or 0,
    /// worldspace, feet, yaw): this guest goes there too (`coop::leash`).
    pub summon: Option<(u32, u32, [f32; 3], f32)>,
    /// The host's: the friendly-fire rule as last sent to the guests (`None`: send it).
    rules_sent: Option<bool>,
    /// The host's: where its player stood at the end of the last frame, so a script's move
    /// takes along only the guests that were within sight of it ([`fnv_game::coop::STORY_NEAR`]).
    host_was: Option<(Place, [f32; 3])>,
    /// The host's: the guests a script's move of the host left behind. The host waits
    /// for them (owner, 2026-11-09: "It shouldn't be a choice the host should have to
    /// wait"): sent as a story wait for the host's own player until each is near
    /// (`scene::share_scene`), while story NPCs are held (`Game::story_gate`).
    pub wait_for: Vec<u8>,
    // The host's.
    shadows: HashMap<u8, Shadow>,
    /// References to send whole to a guest (a digest differed, or the stream started again).
    forced: HashMap<u8, BTreeSet<u32>>,
    /// The host's: when each reference's last change went to each guest. A digest is
    /// late by up to [`DIGEST_EVERY`] and the trip, so a reference that moves all the
    /// time (a tumbleweed in the wind) always differs from the host's copy by then; one
    /// changed within [`SETTLE`] is not a desync (night of 2026-11-09: two tumbleweeds
    /// were sent whole again every digest, a guest's corrections matching its changes).
    sent_at: HashMap<u8, HashMap<u32, f32>>,
    dirty: BTreeSet<FormId>,
    last_tick: f32,
    last_sweep: f32,
    /// The NPCs `share_npcs` found this frame, for the actor stream.
    pub host_npcs: Vec<NpcState>,
    /// Each guest's poses as last sent: when, and the pose.
    poses: HashMap<u8, HashMap<u32, (f32, ActorPose)>>,
    /// What each guest's area held at the last sweep.
    interest: HashMap<u8, usize>,
    changes_sent: HashMap<u8, u64>,
    desyncs: u64,
    effects_sent: u64,
    // The guest's.
    pub inbox: Vec<Event>,
    /// Per actor: what its Look, Anim and Life said.
    slow: HashMap<u32, Slow>,
    last_digest: f32,
    applied: u64,
    corrections: u64,
    /// The guest's: corrections since the last perf line, by reference and component,
    /// so a run of them names what the host keeps changing.
    corrected: HashMap<(u32, u8), u64>,
    poses_heard: u64,
    /// The lowest (here - host) clock difference seen, and the newest round's.
    lag_floor: Option<i64>,
    lag: Vec<f32>,
    effects_played: u64,
    /// A guest's own dropped items (queue item 135) not yet at rest: since when, where it
    /// was last seen and since when it has lain there; and those sent to the host.
    drops_waiting: HashMap<u32, (f32, [f32; 3], f32)>,
    drops_sent: BTreeSet<u32>,
    /// A guest's copies of the players' dropped items in the host's world (base, count),
    /// and those the host's world took away (its own pickup or another player's).
    shared: HashMap<u32, (u32, i32)>,
    host_took: BTreeSet<u32>,
    /// Players' drops made, taken and refused (both sides).
    loose: u64,
    /// The object this guest's physics moves (held, or falling after it let go), the
    /// time it was let go, and when its pose last went to the host.
    carrying: Option<(u32, Option<f32>)>,
    last_carry: f32,
    carries_sent: u64,
    // The host's: guests' carried objects put here.
    carries_heard: u64,
    // Both.
    perf: Perf,
}

#[derive(Default, Clone)]
struct Slow {
    base: u32,
    level: u16,
    all_levels: bool,
    rolls: u8,
    furniture: u32,
    weapon: u32,
    dead: bool,
}

#[derive(Default)]
struct Perf {
    since: Option<f32>,
    frames: Vec<f32>,
    /// Milliseconds spent in replication, per frame.
    spent: Vec<f32>,
    traffic: HashMap<u8, net::Traffic>,
}

impl Perf {
    fn add_spent(&mut self, ms: f32) {
        if let Some(last) = self.spent.last_mut() {
            *last += ms;
        }
    }
}

/// Every frame: the frame time, and a new slot for the replication time spent in it.
pub(super) fn frame_clock(mut coop: ResMut<Coop>, real: Res<Time<Real>>) {
    if coop.link.is_none() && !solo_perf() {
        return;
    }
    let p = &mut coop.repl.perf;
    p.frames.push(real.delta_secs() * 1000.0);
    p.spent.push(0.0);
}

fn percentile(v: &[f32], q: f32) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    let mut s = v.to_vec();
    s.sort_by(f32::total_cmp);
    s[((s.len() - 1) as f32 * q).round() as usize]
}

fn average(v: &[f32]) -> f32 {
    if v.is_empty() {
        0.0
    } else {
        v.iter().sum::<f32>() / v.len() as f32
    }
}

/// The values of reference `r` that every game must agree on (no actor's Look, Life or
/// Anim: those come from the drawn actors): from its state, or its placement when it has
/// none. `None` for a reference neither has.
fn world_values(world: &fnv_sim::World, r: FormId) -> Option<Vec<Value>> {
    let placed = world.placed.get(r);
    let (base, st) = match world.peek(r) {
        Some(st) => (st.base, Some(st)),
        None => (placed?.base, None),
    };
    let data = world.forms.get(base).map(|b| &b.data);
    let actor = matches!(data, Some(fnv_sim::forms::BaseData::Actor(_)));
    let item = matches!(data, Some(fnv_sim::forms::BaseData::Item(_)));
    let mut out = Vec::with_capacity(5);
    let (enabled, deleted) = match st {
        Some(s) => (s.enabled, s.deleted),
        None => (!placed.is_some_and(|p| p.initially_disabled()), false),
    };
    // A player's dropped item is made first on a guest that has not got it (module notes).
    let shared = item && r.is_dynamic() && st.is_some_and(|s| s.player_drop);
    if let (true, Some(s)) = (shared, st) {
        out.push(Value::Item { base: base.0, count: s.count, cell: s.cell.map_or(0, |c| c.0), pos: s.pos, rot: s.rot });
    }
    // A loose item one player took is still there for the others (per-player loot), but
    // not one a player dropped.
    out.push(Value::Enabled { enabled, deleted: deleted && (!item || shared) });
    if !actor {
        let (cell, pos, rot, scale) = match st {
            Some(s) => (s.cell, s.pos, s.rot, s.scale),
            None => {
                let p = placed?;
                (p.cell, p.pos, p.rot, None)
            }
        };
        out.push(Value::Place { cell: cell.map_or(0, |c| c.0), pos, rot, scale: scale.unwrap_or(0.0) });
        if r.is_dynamic() && !item && !matches!(data, Some(fnv_sim::forms::BaseData::Projectile { .. })) {
            out.push(Value::Exists { base: base.0, cell: cell.map_or(0, |c| c.0), pos, rot });
        }
    }
    if placed.is_some_and(|p| p.teleport.is_none()) && matches!(data, Some(fnv_sim::forms::BaseData::Door(_))) {
        out.push(Value::Open(st.map_or(placed.is_some_and(|p| p.open_by_default), |s| s.open)));
    }
    if let Some(l) = world.lock_of(r) {
        out.push(Value::Lock { has: true, locked: l.locked, level: l.level });
    } else if placed.is_some_and(|p| p.lock.is_some()) {
        out.push(Value::Lock { has: false, locked: false, level: 0 });
    }
    Some(out)
}

/// Whether `v` is what the plugins give `r` (both games have it without being told).
fn is_authored(world: &fnv_sim::World, r: FormId, v: &Value) -> bool {
    let Some(p) = world.placed.get(r) else { return false };
    match v {
        Value::Enabled { enabled, deleted } => *enabled == !p.initially_disabled() && !*deleted,
        Value::Place { cell, pos, rot, scale } => *cell == p.cell.map_or(0, |c| c.0) && *pos == p.pos && *rot == p.rot && *scale == 0.0,
        Value::Open(o) => *o == p.open_by_default,
        Value::Lock { has, locked, .. } => *has == p.lock.is_some() && (*locked || !*has),
        _ => false,
    }
}

/// Whether reference `r` is in the area of a player at `place`, `feet`.
fn in_area(world: &fnv_sim::World, r: FormId, place: Place, feet: [f32; 3]) -> bool {
    let (cell, pos) = match world.peek(r) {
        Some(s) => (s.cell, s.pos),
        None => match world.placed.get(r) {
            Some(p) => (p.cell, p.pos),
            None => return false,
        },
    };
    match place {
        Place::Interior(c) => cell.is_some_and(|x| x.0 == c),
        Place::Exterior(w) => cell.and_then(|c| world.placed.world_of(c)).is_some_and(|x| x.0 == w) && (pos[0] - feet[0]).hypot(pos[1] - feet[1]) <= REACH,
    }
}

/// The host's side and the guest's, each frame (module notes).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(super) fn replicate(
    mut commands: Commands,
    time: Res<Time>,
    mut coop: ResMut<Coop>,
    (mut sim, mut script, actor_world, mut residents): (Option<ResMut<nvf_play::sim::SimState>>, Option<ResMut<nvf_play::script::ScriptState>>, Option<Res<ActorWorld>>, Option<ResMut<Residents>>),
    loc: Option<Res<Location>>,
    player: Option<Res<Player>>,
    (grab, mut phys, mut repose): (Option<Res<nvf_play::grab::GrabState>>, Option<ResMut<nvf_play::phys::PhysState>>, MessageWriter<nvf_play::RefRepose>),
) {
    let started = Instant::now();
    let now = time.elapsed_secs();
    let coop = &mut *coop;
    let Some(link) = coop.link.as_ref() else {
        if let Some(s) = sim.as_deref_mut() {
            s.world.set_tracking(false);
        }
        if let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
            rt.game.coop_effects = None;
        }
        if !coop.repl.shadows.is_empty() || !coop.repl.slow.is_empty() {
            coop.repl = Repl::default();
        }
        if let Some(p) = phys.as_deref_mut().filter(|p| !p.follow_only.is_empty()) {
            p.follow_only.clear();
        }
        if solo_perf() {
            perf_line(coop, now);
        }
        return;
    };
    let host = link.is_host();
    let Some(sim) = sim.as_deref_mut() else { return };
    // What this game's physics carries goes to the other players: a guest's to the host,
    // the host's to every guest.
    send_carry(coop, grab.as_deref(), phys.as_deref(), now);
    let moved = if host {
        host_side(coop, sim, script.as_deref_mut(), actor_world.as_deref(), now)
    } else {
        guest_side(&mut commands, coop, sim, script.as_deref_mut(), residents.as_deref_mut(), (loc.as_deref(), player.as_deref(), phys.as_deref()), now)
    };
    // A guest's copies of the objects the host's world moves follow the host: this
    // game's physics does not write them back (`PhysState::follow_only`), or its own
    // simulation of a rolling object would differ from the host's and the desync
    // detector would send it again every digest (night of 2026-11-09). What this guest
    // carries is its own until it is put down.
    if let Some(p) = phys.as_deref_mut() {
        if host {
            if !p.follow_only.is_empty() {
                p.follow_only.clear();
            }
        } else {
            p.follow_only.extend(moved.iter().copied());
            if let Some((id, _)) = coop.repl.carrying {
                p.follow_only.remove(&id);
            }
        }
    }
    for id in moved {
        repose.write(nvf_play::RefRepose { form_id: id });
    }
    let ms = started.elapsed().as_secs_f32() * 1000.0;
    coop.repl.perf.add_spent(ms);
    perf_line(coop, now);
}

/// A guest: the pose of the object its physics moves goes to the host while held, and
/// after it lets go until the body sleeps (a throw, a drop), so the host's game moves its
/// own copy (the host is the world's authority; nothing else tells it).
fn send_carry(coop: &mut Coop, grab: Option<&nvf_play::grab::GrabState>, phys: Option<&nvf_play::phys::PhysState>, now: f32) {
    let (Some(link), Some(phys)) = (coop.link.as_ref(), phys) else { return };
    let r = &mut coop.repl;
    let held = grab.and_then(|g| g.held.as_ref()).and_then(|h| match h.on {
        fnv_phys::GripOn::Ref(id) => Some(id),
        fnv_phys::GripOn::Ragdoll(_) => None,
    });
    match (held, r.carrying) {
        (Some(id), Some((was, _))) if id == was => r.carrying = Some((id, None)),
        (Some(id), _) => {
            info!("replication: carrying {id:#010X}: its pose goes to the other players");
            r.carrying = Some((id, None));
        }
        (None, Some((id, None))) => r.carrying = Some((id, Some(now))),
        _ => {}
    }
    let Some((id, let_go)) = r.carrying else { return };
    if now - r.last_carry < CARRY_EVERY {
        return;
    }
    r.last_carry = now;
    let Some((pos, rot, sleeping)) = phys.ref_pose_now(id) else {
        r.carrying = None;
        return;
    };
    let done = let_go.is_some_and(|t| sleeping || now - t >= CARRY_FALL_MAX);
    link.send_reliable(None, repl::pack_carry(&repl::Carry { reference: id, pos, rot, done }));
    r.carries_sent += 1;
    if done {
        info!("replication: {id:#010X} at rest at {pos:?}: its last pose went to the other players");
        r.carrying = None;
    }
}

/// The host's side; the references a guest's carry moved (their collision follows).
fn host_side(coop: &mut Coop, sim: &mut nvf_play::sim::SimState, mut script: Option<&mut nvf_play::script::ScriptState>, aw: Option<&ActorWorld>, now: f32) -> Vec<u32> {
    let mut moved = Vec::new();
    let Some(link) = coop.link.as_ref() else { return moved };
    let r = &mut coop.repl;
    let guests: Vec<(u8, Place, [f32; 3], f32)> = coop.peers.iter().filter(|(_, p)| !p.lost).filter_map(|(&id, p)| p.samples.back().map(|(_, s)| (id, s.place, s.feet, s.yaw))).collect();
    let game = script.as_deref_mut().and_then(|s| s.rt.as_mut()).map(|rt| &mut rt.game);
    if guests.is_empty() {
        sim.world.set_tracking(false);
        r.dirty.clear();
        if let Some(g) = game {
            g.coop_effects = None;
        }
        return moved;
    }
    sim.world.set_tracking(true);
    // The script effects the host's scripts asked for go to every guest (§7, phase 4).
    let mut summons: Vec<(Effect, Vec<u8>)> = Vec::new();
    if let Some(g) = game {
        let asked = std::mem::take(g.coop_effects.get_or_insert_with(Vec::new));
        let mut effects: Vec<Effect> = asked
            .into_iter()
            .filter_map(|e| match e {
                fnv_game::sound::CoopEffect::Sound { sound, at } => Some(Effect::Sound { sound: sound.0, at_ref: at.0, pos: [0.0; 3] }),
                fnv_game::sound::CoopEffect::Music(m) => Some(Effect::Music { music: m.0 }),
                fnv_game::sound::CoopEffect::PlayerMoved => {
                    // Where the script put the host's player.
                    let s = sim.world.peek(FormId::PLAYER_REF)?;
                    let cell = s.cell?;
                    let summon = match sim.world.placed.world_of(cell) {
                        Some(w) => Effect::Summon { interior: 0, world: w.0, pos: s.pos, yaw: s.rot[2] },
                        None => Effect::Summon { interior: cell.0, world: 0, pos: s.pos, yaw: s.rot[2] },
                    };
                    // Only the guests within sight of where the host stood go along
                    // (owner, 2026-11-09: a cutscene pulls in the players within sight).
                    let (near, far) = near_guests(r.host_was, &guests);
                    let names = |ids: &[u8]| ids.iter().map(|i| format!("player {}", i + 1)).collect::<Vec<_>>().join(", ");
                    info!(
                        "replication: a script moved the host to {cell} {:?}: {} go with it{}",
                        s.pos,
                        if near.is_empty() { "no guest within sight".to_string() } else { names(&near) },
                        if far.is_empty() { String::new() } else { format!("; {} too far, left where they are", names(&far)) }
                    );
                    if !near.is_empty() {
                        summons.push((summon, near));
                    }
                    if !far.is_empty() {
                        info!("co-op: story wait: the host waits for {}", names(&far));
                        for id in far {
                            if !r.wait_for.contains(&id) {
                                r.wait_for.push(id);
                            }
                        }
                    }
                    None
                }
            })
            .collect();
        // The lobby's rules (item 122): when they change, and again for a stream started anew.
        if r.rules_sent != Some(coop.friendly_fire) {
            r.rules_sent = Some(coop.friendly_fire);
            info!("replication: the rules go to the guests: friendly fire {}", if coop.friendly_fire { "on" } else { "off" });
            effects.push(Effect::Rules { friendly_fire: coop.friendly_fire });
        }
        if !effects.is_empty() {
            r.effects_sent += effects.len() as u64;
            for item in repl::pack_effects(&effects, ITEM) {
                link.send_reliable(None, item);
            }
        }
        for (summon, to) in summons {
            r.effects_sent += 1;
            for id in to {
                for item in repl::pack_effects(std::slice::from_ref(&summon), ITEM) {
                    link.send_reliable(Some(id), item);
                }
            }
        }
    }
    r.host_was = sim.world.peek(FormId::PLAYER_REF).and_then(|s| {
        let c = s.cell?;
        Some((match sim.world.placed.world_of(c) {
            Some(w) => Place::Exterior(w.0),
            None => Place::Interior(c.0),
        }, s.pos))
    });
    for e in std::mem::take(&mut r.inbox) {
        match e {
            Event::RelReset { id } => {
                r.rules_sent = None;
                r.shadows.remove(&id);
                r.sent_at.remove(&id);
                r.poses.remove(&id);
                info!("replication: player {} starts again: the whole area goes to it", id + 1);
            }
            Event::Reliable { id, item } => {
                if let Some(c) = repl::unpack_carry(&item) {
                    // A guest carries this object: the host's copy goes where the guest
                    // has it; the other guests get it by the change log (the write
                    // touches it); the carrier is not sent its own move back.
                    let rf = FormId(c.reference);
                    let Ok(s) = sim.world.get(rf) else { continue };
                    s.pos = c.pos;
                    s.rot = c.rot;
                    if !moved.contains(&c.reference) {
                        moved.push(c.reference);
                    }
                    r.carries_heard += 1;
                    if c.done {
                        info!("replication: player {} put {} {rf} down at {:?}", id + 1, sim.ref_name(None, rf.0).unwrap_or("?"), c.pos);
                    }
                    if let Some(values) = world_values(&sim.world, rf) {
                        let shadow = r.shadows.entry(id).or_default();
                        for v in values.iter().filter(|v| v.comp() == comp::PLACE) {
                            shadow.take(rf.0, v);
                        }
                    }
                } else if let Some(l) = repl::unpack_loose(&item) {
                    host_loose(link, r, sim, id, l);
                } else if let Some(d) = repl::unpack_digest(&item) {
                    let shadow = r.shadows.entry(id).or_default();
                    let sent_at = r.sent_at.entry(id).or_default();
                    for (reference, theirs) in d.refs {
                        let rf = FormId(reference);
                        let Some(mine) = world_values(&sim.world, rf) else { continue };
                        if sent_at.get(&reference).is_some_and(|&t| now - t < SETTLE) {
                            continue;
                        }
                        if repl::digest_of(&mine) != theirs {
                            r.desyncs += 1;
                            let what: Vec<String> = mine.iter().filter(|v| !shadow.of(reference).any(|s| s == *v)).map(|v| format!("{v:?}")).collect();
                            info!("replication desync: player {} holds {} {rf} unlike the host ({}): sent again", id + 1, sim.ref_name(None, rf.0).unwrap_or("?"), if what.is_empty() { "its own change".to_string() } else { what.join(", ") });
                            shadow.forget(reference);
                            r.forced.entry(id).or_default().insert(reference);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if now - r.last_tick < TICK {
        return moved;
    }
    r.last_tick = now;
    r.dirty.extend(sim.world.take_touched());
    let sweep = now - r.last_sweep >= SWEEP;
    if sweep {
        r.last_sweep = now;
    }
    let dead = |x: FormId| script.as_deref().and_then(|s| s.rt.as_ref()).is_some_and(|rt| rt.game.is_dead(x));
    let rolls = aw.and_then(|w| w.rolls.lock().ok().map(|m| m.clone())).unwrap_or_default();
    for &(id, place, feet, yaw) in &guests {
        let shadow = r.shadows.entry(id).or_default();
        let forced = r.forced.remove(&id).unwrap_or_default();
        let mut changes = Vec::new();
        let mut consider = |rf: FormId, changes: &mut Vec<Change>, force: bool| {
            let Some(values) = world_values(&sim.world, rf) else { return };
            for v in values {
                let new = shadow.of(rf.0).all(|s| s.comp() != v.comp());
                if !force && new && is_authored(&sim.world, rf, &v) {
                    continue;
                }
                if shadow.take(rf.0, &v) || force {
                    changes.push(Change { reference: rf.0, value: v });
                }
            }
        };
        let mut seen = 0usize;
        if sweep {
            let refs: Vec<FormId> = sim.world.changed().map(|(k, _)| k).filter(|&k| k != FormId::PLAYER_REF).collect();
            for rf in refs {
                if in_area(&sim.world, rf, place, feet) {
                    seen += 1;
                    consider(rf, &mut changes, forced.contains(&rf.0));
                }
            }
            r.interest.insert(id, seen);
        } else {
            for &rf in r.dirty.iter().filter(|&&k| k != FormId::PLAYER_REF) {
                if in_area(&sim.world, rf, place, feet) {
                    consider(rf, &mut changes, forced.contains(&rf.0));
                }
            }
        }
        for &f in forced.iter().filter(|&&f| !r.dirty.contains(&FormId(f))) {
            if !sweep {
                consider(FormId(f), &mut changes, true);
            }
        }
        // The actors the host draws: Life and Anim each tick, Look on a sweep.
        for n in &r.host_npcs {
            let rf = FormId(n.reference);
            let mut vals = vec![Value::Life { dead: n.has(npc_flag::DEAD) || dead(rf) }, Value::Anim { furniture: n.furniture, weapon: n.weapon }];
            if sweep || shadow.of(n.reference).all(|s| s.comp() != comp::LOOK) {
                let worn = sim.world.equipment(rf).unwrap_or_default().into_iter().map(|w| w.0).collect();
                let level = aw.map_or(fnv_sim::zones::LevelAt::plain(0), |w| w.level(n.reference));
                vals.push(Value::Look { base: n.base, level: level.level, all_levels: level.all_levels, rolls: rolls.get(&n.reference).copied().unwrap_or(0).min(127) as u8, worn });
            }
            for v in vals {
                if shadow.take(n.reference, &v) {
                    changes.push(Change { reference: n.reference, value: v });
                }
            }
        }
        if !changes.is_empty() {
            let sent_at = r.sent_at.entry(id).or_default();
            sent_at.retain(|_, t| now - *t < SETTLE);
            for c in &changes {
                sent_at.insert(c.reference, now);
            }
            *r.changes_sent.entry(id).or_default() += changes.len() as u64;
            for item in repl::pack_changes(&changes, ITEM) {
                link.send_reliable(Some(id), item);
            }
        }
        // The compact actor stream.
        let sent = r.poses.entry(id).or_default();
        let mut poses = Vec::new();
        let eye = [feet[0], feet[1], feet[2] + 120.0];
        for n in &r.host_npcs {
            let pose = ActorPose { reference: n.reference, feet: n.feet, heading: n.heading, pitch: n.pitch, flags: n.flags, attacks: n.attacks }.quantised();
            let d = (n.feet[0] - feet[0]).hypot(n.feet[1] - feet[1]);
            let hz = repl::pose_hz(d, repl::in_view(eye, yaw, n.feet));
            let go = match sent.get(&n.reference) {
                None => true,
                Some((at, last)) => {
                    let age = now - at;
                    (age + 1e-3 >= 1.0 / hz && *last != pose) || age >= KEEP_ALIVE
                }
            };
            if go {
                sent.insert(n.reference, (now, pose));
                poses.push(pose);
            }
        }
        sent.retain(|_, (at, _)| now - *at < 10.0);
        if !poses.is_empty() {
            link.send_actors(id, &poses);
        }
    }
    r.dirty.clear();
    moved
}

#[allow(clippy::too_many_arguments)]
fn guest_side(commands: &mut Commands, coop: &mut Coop, sim: &mut nvf_play::sim::SimState, mut script: Option<&mut nvf_play::script::ScriptState>, mut residents: Option<&mut Residents>, (loc, player, phys): (Option<&Location>, Option<&Player>, Option<&nvf_play::phys::PhysState>), now: f32) -> Vec<u32> {
    // The references whose place changed here (their collision follows).
    let mut moved = Vec::new();
    let Some(link) = coop.link.as_ref() else { return moved };
    let r = &mut coop.repl;
    for e in std::mem::take(&mut r.inbox) {
        match e {
            Event::RelReset { .. } => info!("replication: the host's stream starts again"),
            Event::Reliable { item, .. } => {
                if let Some(changes) = repl::unpack_changes(&item) {
                    for c in changes {
                        // What this game's physics carries is its own until it is put
                        // down (the host follows it, `send_carry`).
                        if r.carrying.is_some_and(|(id, _)| id == c.reference) && c.value.comp() == comp::PLACE {
                            continue;
                        }
                        r.applied += 1;
                        if apply(sim, &mut r.slow, &c, residents.as_deref_mut(), commands) {
                            r.corrections += 1;
                            *r.corrected.entry((c.reference, c.value.comp())).or_default() += 1;
                            if matches!(c.value.comp(), comp::PLACE | comp::ITEM) && !moved.contains(&c.reference) {
                                moved.push(c.reference);
                            }
                        }
                        match c.value {
                            // Not one this player has already taken (a digest sends it again).
                            Value::Item { base, count, .. } if !sim.world.peek(FormId(c.reference)).is_some_and(|s| s.deleted) => {
                                r.shared.insert(c.reference, (base, count));
                            }
                            Value::Enabled { deleted: true, .. } if r.shared.contains_key(&c.reference) => {
                                r.host_took.insert(c.reference);
                            }
                            _ => {}
                        }
                    }
                } else if let Some(c) = repl::unpack_carry(&item) {
                    // The host carries this object: this game's copy goes where the
                    // host has it (not one this game holds itself).
                    if r.carrying.is_some_and(|(id, _)| id == c.reference) {
                        continue;
                    }
                    let rf = FormId(c.reference);
                    let Ok(s) = sim.world.get(rf) else { continue };
                    s.pos = c.pos;
                    s.rot = c.rot;
                    r.carries_heard += 1;
                    if !moved.contains(&c.reference) {
                        moved.push(c.reference);
                    }
                    if c.done {
                        info!("replication: the host put {} {rf} down at {:?}", sim.ref_name(None, rf.0).unwrap_or("?"), c.pos);
                    }
                } else if let Some(l) = repl::unpack_loose(&item) {
                    guest_loose(link, r, sim, l, &mut moved);
                } else if let Some(effects) = repl::unpack_effects(&item) {
                    // Played as this game's own scripts' would be (the sound at the
                    // reference, which this game has too).
                    if let Some(g) = script.as_deref_mut().and_then(|s| s.rt.as_mut()).map(|rt| &mut rt.game) {
                        for e in effects {
                            r.effects_played += 1;
                            match e {
                                Effect::Sound { sound, at_ref, .. } => g.sound_requests.push(fnv_game::sound::SoundRequest { sound: FormId(sound), at: Some(FormId(at_ref)), system: false, id: 0 }),
                                Effect::Music { music } => g.music_requests.push(FormId(music)),
                                Effect::Summon { interior, world, pos, yaw } => {
                                    info!("replication: the host's script moved it: this player goes too");
                                    r.summon = Some((interior, world, pos, yaw));
                                }
                                Effect::Rules { friendly_fire } => {
                                    if coop.friendly_fire != friendly_fire {
                                        info!("replication: the host's rules: friendly fire {}", if friendly_fire { "on" } else { "off" });
                                    }
                                    coop.friendly_fire = friendly_fire;
                                }
                            }
                        }
                    }
                }
            }
            Event::Actors { ms, here_ms, poses } => {
                let off = i64::from(here_ms) - i64::from(ms);
                let floor = *r.lag_floor.get_or_insert(off);
                if off < floor {
                    r.lag_floor = Some(off);
                }
                r.lag.push((off - r.lag_floor.unwrap_or(off)) as f32);
                r.poses_heard += poses.len() as u64;
                for p in poses {
                    let s = r.slow.get(&p.reference).cloned().unwrap_or_default();
                    let flags = p.flags | if s.dead { npc_flag::DEAD } else { 0 };
                    let n = NpcState {
                        reference: p.reference,
                        feet: p.feet,
                        heading: p.heading,
                        pitch: p.pitch,
                        flags,
                        weapon: s.weapon,
                        attacks: p.attacks,
                        furniture: s.furniture,
                        base: s.base,
                        level: s.level,
                        all_levels: s.all_levels,
                        rolls: s.rolls,
                    };
                    let t = coop.npcs.entry(p.reference).or_default();
                    t.samples.push_back((now, n));
                    while t.samples.len() > 8 {
                        t.samples.pop_front();
                    }
                }
            }
            _ => {}
        }
    }
    guest_drops(link, r, sim, phys, now);
    // The digest of what this game holds near its player.
    if now - r.last_digest >= DIGEST_EVERY {
        r.last_digest = now;
        let (Some(loc), Some(player)) = (loc, player) else { return moved };
        let place = place_of(loc);
        let feet = player.feet;
        // Not this game's own created references, nor its actors hidden as not in the
        // host's world here (`not_in_hosts_world`).
        let hidden = &coop.hidden;
        let refs: Vec<FormId> = sim.world.changed().map(|(k, _)| k).filter(|&k| k != FormId::PLAYER_REF && k.0 < GUEST_CREATED && !hidden.contains(&k)).collect();
        let mut d = Digest { refs: Vec::new() };
        for rf in refs {
            if in_area(&sim.world, rf, place, feet) {
                if let Some(v) = world_values(&sim.world, rf) {
                    d.refs.push((rf.0, repl::digest_of(&v)));
                }
            }
        }
        for item in repl::pack_digest(&d, ITEM) {
            link.send_reliable(None, item);
        }
    }
    moved
}

/// Apply one of the host's changes here; whether it changed anything.
fn apply(sim: &mut nvf_play::sim::SimState, slow: &mut HashMap<u32, Slow>, c: &Change, residents: Option<&mut Residents>, commands: &mut Commands) -> bool {
    let rf = FormId(c.reference);
    let w = &mut sim.world;
    match &c.value {
        Value::Enabled { enabled, deleted } => {
            let item = w.peek(rf).map(|s| s.base).or_else(|| w.placed.get(rf).map(|p| p.base)).and_then(|b| w.forms.item(b)).is_some();
            let shared = w.peek(rf).is_some_and(|s| s.player_drop);
            let Ok(st) = w.get(rf) else { return false };
            let was = (st.enabled, st.deleted);
            let _ = w.set_enabled(rf, *enabled);
            // A loose item this player took stays taken here (per-player loot); a player's
            // dropped item is the host's world's.
            if !item || shared {
                let _ = w.set_deleted(rf, *deleted);
            }
            w.peek(rf).is_some_and(|s| (s.enabled, s.deleted) != was)
        }
        Value::Place { cell, pos, rot, scale } => {
            let Ok(st) = w.get(rf) else { return false };
            let new_cell = (*cell != 0).then_some(FormId(*cell));
            let new_scale = (*scale != 0.0).then_some(*scale);
            let changed = st.cell != new_cell || st.pos != *pos || st.rot != *rot || st.scale != new_scale;
            st.cell = new_cell;
            st.pos = *pos;
            st.rot = *rot;
            st.scale = new_scale;
            changed
        }
        Value::Open(o) => {
            let Ok(st) = w.get(rf) else { return false };
            let changed = st.open != *o;
            st.open = *o;
            changed
        }
        Value::Lock { has, locked, level } => {
            let was = w.lock_of(rf).map(|l| (l.locked, l.level));
            let _ = match (has, locked) {
                (true, true) => w.lock(rf, Some(*level)),
                _ => w.unlock(rf),
            };
            w.lock_of(rf).map(|l| (l.locked, l.level)) != was
        }
        Value::Exists { base, cell, pos, rot } => {
            if w.peek(rf).is_some() {
                return false;
            }
            match w.place_as(rf, FormId(*base), (*cell != 0).then_some(FormId(*cell)), *pos, *rot) {
                Ok(_) => {
                    info!("replication: the host's {} {rf} made here", w.forms.get(FormId(*base)).and_then(|b| b.edid.clone()).unwrap_or_default());
                    true
                }
                Err(e) => {
                    warn!("replication: the host's {rf} cannot be made here: {e:?}");
                    false
                }
            }
        }
        Value::Item { base, count, cell, pos, rot } => {
            if let Some(had) = w.peek(rf).map(|s| s.count) {
                let Ok(st) = w.get(rf) else { return false };
                st.count = *count;
                st.player_drop = true;
                return had != *count;
            }
            match w.place_as(rf, FormId(*base), (*cell != 0).then_some(FormId(*cell)), *pos, *rot) {
                Ok(_) => {
                    if let Ok(st) = w.get(rf) {
                        st.count = *count;
                        st.player_drop = true;
                    }
                    info!("replication: a player's dropped {} x{count} {rf} lies here too", w.forms.get(FormId(*base)).and_then(|b| b.edid.clone()).unwrap_or_default());
                    true
                }
                Err(e) => {
                    warn!("replication: a player's dropped {rf} cannot be made here: {e:?}");
                    false
                }
            }
        }
        Value::Life { dead } => {
            let s = slow.entry(c.reference).or_default();
            let changed = s.dead != *dead;
            s.dead = *dead;
            changed
        }
        Value::Anim { furniture, weapon } => {
            let s = slow.entry(c.reference).or_default();
            let changed = (s.furniture, s.weapon) != (*furniture, *weapon);
            (s.furniture, s.weapon) = (*furniture, *weapon);
            changed
        }
        Value::Look { base, level, all_levels, rolls, worn } => {
            let s = slow.entry(c.reference).or_default();
            (s.base, s.level, s.all_levels, s.rolls) = (*base, *level, *all_levels, *rolls);
            let items: Vec<FormId> = worn.iter().map(|&i| FormId(i)).collect();
            let dressed = w.wear_as(rf, &items).unwrap_or(false);
            if dressed {
                info!("replication: {rf} dressed as in the host's game ({} items)", items.len());
                if let Some(res) = residents {
                    res.redraw(commands, c.reference);
                }
            }
            dressed
        }
    }
}

/// How long a guest's dropped item lies still (within [`DROP_STILL`] units) before it goes
/// to the host as at rest: its body need not sleep (a dropped pistol lay still for 10 s
/// without sleeping in the first two-window run, 2026-11-09).
const DROP_REST: f32 = 0.5;
const DROP_STILL: f32 = 1.0;

/// The host: a guest's dropped item or pickup (queue item 135, module notes).
fn host_loose(link: &Link, r: &mut Repl, sim: &mut nvf_play::sim::SimState, id: u8, l: Loose) {
    match l {
        Loose::Dropped { local, base, count, cell, pos, rot } => match sim.world.place(FormId(base), (cell != 0).then_some(FormId(cell)), pos, rot, count) {
            Ok(made) => {
                if let Ok(s) = sim.world.get(made) {
                    s.player_drop = true;
                }
                r.loose += 1;
                info!("replication: player {} dropped {} x{count}: made here as {made}, every player sees it", id + 1, sim.name(FormId(base)).unwrap_or("?"));
                link.send_reliable(Some(id), repl::pack_loose(&Loose::Made { local, host: made.0 }));
            }
            Err(e) => warn!("replication: player {}'s dropped {base:#010X} cannot be made here: {e:?}", id + 1),
        },
        Loose::Took { reference } => {
            let rf = FormId(reference);
            let lying = sim.world.peek(rf).filter(|s| s.player_drop).map(|s| (s.base.0, s.count, !s.deleted && s.enabled));
            r.loose += 1;
            match lying {
                Some((_, count, true)) => {
                    let _ = sim.world.set_deleted(rf, true);
                    info!("replication: player {} took {} x{count} {rf}: gone for every player", id + 1, sim.ref_name(None, reference).unwrap_or("?"));
                }
                Some((base, count, false)) => {
                    info!("replication: player {} took {} {rf}, but it was already taken: it gives it back", id + 1, sim.ref_name(None, reference).unwrap_or("?"));
                    link.send_reliable(Some(id), repl::pack_loose(&Loose::Refused { reference, base, count }));
                }
                None => warn!("replication: player {} took {rf}, which is no player's dropped item here", id + 1),
            }
        }
        Loose::Made { .. } | Loose::Refused { .. } => {}
    }
}

/// A guest: the host's answer about its drop or pickup.
fn guest_loose(link: &Link, r: &mut Repl, sim: &mut nvf_play::sim::SimState, l: Loose, moved: &mut Vec<u32>) {
    match l {
        Loose::Made { local, host } => {
            let (lf, hf) = (FormId(local), FormId(host));
            r.loose += 1;
            match sim.world.peek(lf).filter(|s| !s.deleted && s.enabled).cloned() {
                Some(s) => {
                    // This game's copy takes the host's number: the same item for everyone.
                    let _ = sim.world.set_deleted(lf, true);
                    if sim.world.peek(hf).is_none() && sim.world.place_as(hf, s.base, s.cell, s.pos, s.rot).is_ok() {
                        if let Ok(n) = sim.world.get(hf) {
                            n.count = s.count;
                            n.extra = s.extra;
                            n.player_drop = true;
                        }
                    }
                    r.shared.insert(host, (s.base.0, s.count));
                    if !moved.contains(&host) {
                        moved.push(host);
                    }
                    info!("replication: this player's dropped {} {lf} is {hf} in the host's world", sim.name(s.base).unwrap_or("?"));
                }
                None => {
                    // Taken back here before the host had it: taken there too.
                    info!("replication: this player's dropped {lf} was taken back before the host made it ({hf}): taken there too");
                    link.send_reliable(None, repl::pack_loose(&Loose::Took { reference: host }));
                }
            }
        }
        Loose::Refused { reference, base, count } => {
            r.loose += 1;
            let back = sim.world.remove_item(FormId::PLAYER_REF, FormId(base), count).unwrap_or(0);
            info!("replication: another player took {} {reference:#010X} first: {back} of {count} given back", sim.name(FormId(base)).unwrap_or("?"));
        }
        Loose::Dropped { .. } | Loose::Took { .. } => {}
    }
}

/// A guest: its own drops that came to rest go to the host, and its pickups of the
/// players' dropped items are asked of it (module notes).
fn guest_drops(link: &Link, r: &mut Repl, sim: &nvf_play::sim::SimState, phys: Option<&nvf_play::phys::PhysState>, now: f32) {
    type Mine = (FormId, u32, i32, u32, [f32; 3], [f32; 3]);
    let mine: Vec<Mine> = sim
        .world
        .created()
        .filter(|(k, s)| k.0 >= GUEST_CREATED && s.player_drop && !s.deleted && s.enabled && !r.drops_sent.contains(&k.0))
        .map(|(k, s)| (k, s.base.0, s.count, s.cell.map_or(0, |c| c.0), s.pos, s.rot))
        .collect();
    r.drops_waiting.retain(|k, _| mine.iter().any(|m| m.0 .0 == *k));
    for (k, base, count, cell, pos, rot) in mine {
        let (pos, rot, sleeping) = phys.and_then(|p| p.ref_pose_now(k.0)).unwrap_or((pos, rot, false));
        let w = r.drops_waiting.entry(k.0).or_insert((now, pos, now));
        if (w.1[0] - pos[0]).hypot(w.1[1] - pos[1]).hypot(w.1[2] - pos[2]) > DROP_STILL {
            (w.1, w.2) = (pos, now);
        }
        let (since, still) = (w.0, w.2);
        if !sleeping && now - still < DROP_REST && now - since < CARRY_FALL_MAX {
            continue;
        }
        link.send_reliable(None, repl::pack_loose(&Loose::Dropped { local: k.0, base, count, cell, pos, rot }));
        r.drops_sent.insert(k.0);
        r.drops_waiting.remove(&k.0);
        r.loose += 1;
        info!("replication: this player dropped {} x{count} {k}, at rest at {pos:?}: it goes to the host", sim.name(FormId(base)).unwrap_or("?"));
    }
    let gone: Vec<u32> = r.shared.keys().copied().filter(|&k| sim.world.peek(FormId(k)).is_none_or(|s| s.deleted || !s.enabled)).collect();
    for k in gone {
        r.shared.remove(&k);
        if r.host_took.remove(&k) {
            continue;
        }
        r.loose += 1;
        info!("replication: this player took the dropped {} {k:#010X}: asked of the host", sim.ref_name(None, k).unwrap_or("?"));
        link.send_reliable(None, repl::pack_loose(&Loose::Took { reference: k }));
    }
}

fn perf_line(coop: &mut Coop, now: f32) {
    let r = &mut coop.repl;
    let Some(link) = coop.link.as_ref() else {
        // The solo baseline ([`PERF_ENV`]).
        let since = *r.perf.since.get_or_insert(now);
        if now - since >= PERF_EVERY {
            let f = &r.perf.frames;
            info!("replication perf: solo frame avg {:.2} ms p95 {:.2} p99 {:.2} ({} frames)", average(f), percentile(f, 0.95), percentile(f, 0.99), f.len());
            r.perf.frames.clear();
            r.perf.spent.clear();
            r.perf.since = Some(now);
        }
        return;
    };
    let since = *r.perf.since.get_or_insert(now);
    let span = now - since;
    if span < PERF_EVERY {
        return;
    }
    let traffic = link.traffic();
    let rate = |id: u8, f: fn(&net::Traffic) -> u64, t: &net::Traffic| {
        let before = r.perf.traffic.get(&id).map_or(0, f);
        (f(t).saturating_sub(before)) as f32 / span / 1024.0
    };
    let frames = &r.perf.frames;
    let (avg, p95, p99) = (average(frames), percentile(frames, 0.95), percentile(frames, 0.99));
    let (spent_avg, spent_max) = (average(&r.perf.spent), r.perf.spent.iter().copied().fold(0.0, f32::max));
    let mut line = format!(
        "replication perf: {} frame avg {avg:.2} ms p95 {p95:.2} p99 {p99:.2} ({} frames); replication {spent_avg:.3} ms/frame (max {spent_max:.2})",
        if link.is_host() { "host" } else { "guest" },
        frames.len()
    );
    if link.is_host() {
        for t in &traffic {
            line += &format!(
                "; player {}: sent {:.2} KB/s heard {:.2} KB/s, area {} refs, actors {}, changes {}, pending {}, resent {}",
                t.id + 1,
                rate(t.id, |t| t.sent, t),
                rate(t.id, |t| t.heard, t),
                r.interest.get(&t.id).copied().unwrap_or(0),
                r.host_npcs.len(),
                r.changes_sent.get(&t.id).copied().unwrap_or(0),
                t.pending,
                t.resent
            );
        }
        line += &format!("; desyncs {}, effects {}, players' drops {}", r.desyncs, r.effects_sent, r.loose);
    } else if let Some(t) = traffic.first() {
        line += &format!(
            "; from host: heard {:.2} KB/s sent {:.2} KB/s, changes {} (corrections {}), poses {:.0}/s, snapshot lag avg {:.0} ms p95 {:.0} ms (over the lowest seen), effects {}, players' drops {}",
            rate(0, |t| t.heard, t),
            rate(0, |t| t.sent, t),
            r.applied,
            r.corrections,
            r.poses_heard as f32 / span,
            average(&r.lag),
            percentile(&r.lag, 0.95),
            r.effects_played,
            r.loose
        );
        if !r.corrected.is_empty() {
            let mut most: Vec<((u32, u8), u64)> = r.corrected.drain().collect();
            most.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            let top: Vec<String> = most.iter().take(3).map(|((id, c), n)| format!("{id:08X} {} x{n}", comp_name(*c))).collect();
            line += &format!("; most corrected since the last line: {}", top.join(", "));
        }
        r.poses_heard = 0;
        r.lag.clear();
    }
    info!("{line}");
    if std::env::var(HUD_ENV).is_ok_and(|v| v == "1") {
        coop.repl_hud = Some(line);
    }
    let r = &mut coop.repl;
    r.perf.traffic = traffic.into_iter().map(|t| (t.id, t)).collect();
    r.perf.frames.clear();
    r.perf.spent.clear();
    r.perf.since = Some(now);
}

/// The optional corner readout ([`HUD_ENV`]): the newest perf line, top right.
#[derive(Component)]
pub(super) struct ReplHud;

pub(super) fn hud(mut commands: Commands, mut coop: ResMut<Coop>, mut shown: Query<&mut Text, With<ReplHud>>) {
    let Some(line) = coop.repl_hud.take() else { return };
    let text = line.replace("; ", "\n");
    match shown.single_mut() {
        Ok(mut t) => t.0 = text,
        Err(_) => {
            commands.spawn((
                ReplHud,
                Text::new(text),
                TextFont { font_size: FontSize::Px(11.0), ..default() },
                TextColor(Color::srgba(1.0, 0.85, 0.5, 0.9)),
                Node { position_type: PositionType::Absolute, top: Val::Px(8.0), right: Val::Px(8.0), max_width: Val::Px(520.0), ..default() },
            ));
        }
    }
}

/// A replicated component's name, for the perf line.
fn comp_name(c: u8) -> &'static str {
    match c {
        comp::ENABLED => "enabled",
        comp::PLACE => "place",
        comp::OPEN => "open",
        comp::LOCK => "lock",
        comp::LIFE => "life",
        comp::LOOK => "look",
        comp::EXISTS => "exists",
        comp::ANIM => "anim",
        comp::ITEM => "item",
        _ => "?",
    }
}

/// A guest, where it is: its id, place, feet and yaw.
type GuestAt = (u8, Place, [f32; 3], f32);

/// The guests within [`fnv_game::coop::STORY_NEAR`] units of where the host stood (same
/// place), and the others. No place known for the host: every guest is near (as before).
fn near_guests(host: Option<(Place, [f32; 3])>, guests: &[(u8, Place, [f32; 3], f32)]) -> (Vec<u8>, Vec<u8>) {
    let near = |place: Place, feet: [f32; 3]| {
        host.is_none_or(|(hp, hf)| hp == place && (0..3).map(|i| (feet[i] - hf[i]).powi(2)).sum::<f32>() <= fnv_game::coop::STORY_NEAR * fnv_game::coop::STORY_NEAR)
    };
    let (n, f): (Vec<&GuestAt>, Vec<_>) = guests.iter().partition(|g| near(g.1, g.2));
    (n.iter().map(|g| g.0).collect(), f.iter().map(|g| g.0).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A script's move of the host takes the guests within sight of where it stood
    /// (owner, 2026-11-09), not one in another place or farther; with no place known for
    /// the host, every guest, as before.
    #[test]
    fn a_scripted_move_takes_the_guests_within_sight() {
        let here = Place::Interior(0x0001_2345);
        let guests = [(0u8, here, [500.0, 0.0, 0.0], 0.0), (1, here, [3000.0, 0.0, 0.0], 0.0), (2, Place::Exterior(0xDA726), [0.0; 3], 0.0)];
        assert_eq!(near_guests(Some((here, [0.0; 3])), &guests), (vec![0], vec![1, 2]));
        assert_eq!(near_guests(None, &guests).0, vec![0, 1, 2]);
    }
}
