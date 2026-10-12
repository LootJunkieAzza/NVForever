//! Idle animations played (docs/spec-notes/idles.md): what the game's rules ask for
//! (`fnv_game::idles`: a script's `PlayIdle`, a package's begin, end and change idles,
//! the idles on a dialogue response) and the idle collections of packages and idle
//! markers, on the actors drawn and on the player.
//!
//! - **Requests.** Each frame the game's `idle_requests` are taken and started: the
//!   idle's `.kf` is loaded, and how often it loops is rolled from its record
//!   (`IdleDef::loops`). An actor that is not drawn plays nothing.
//! - **Collections.** An actor standing where its package has it, with nothing else to
//!   do (`Residents::standing`), plays the idles of its package's collection, or of the
//!   idle marker it stands at (which override the package's [documented]), picked by
//!   `fnv_game::idles::IdlePicker`; so does the player under a script package. The pick
//!   is asked of the game like any other idle, so it is traced. A collection idle ends
//!   when its actor walks off.
//! - **Playing.** `fnv_actor::idle_play::IdlePlay` keeps each idle's time, loops and
//!   fades. On an NPC the clip is laid over what it plays otherwise (its locomotion, or
//!   its furniture clip) by track priority, on two more blend layers ([`ActorIdle`],
//!   read by `locomotion::drive`): the idle, and the one before it while the new one
//!   fades in. On the player ([`PlayerIdles`], read by `player_body.rs`) an idle whose
//!   clip drives the first-person camera node plays on the first-person rig and turns
//!   the view; any other plays on the third-person body.
//! - **Ends.** A one-shot idle ends at the end of its clip. An idle that loops for ever
//!   is replaced by the next idle asked for, and leaves its loop when the package (or
//!   marker) that asked for it is no longer the actor's.
//! - The rules are told what each actor plays (`Game::set_idle_playing`), which is what
//!   `IsIdlePlaying` answers.
//! - The slot also plays the animation groups scripts ask of actors (`PlayGroup`,
//!   `actor_groups.rs`); those are not idles for `IsIdlePlaying`.
//!
//! Not done: the idle manager (idles chosen from the IDLE tree by their conditions:
//! `PickIdle`, fidgets, talking gestures), the sounds and other text keys of idle clips,
//! and package idles of actors that are not residents. The props idles hold (ANIO) are
//! drawn by `anim_objects.rs`.

use crate::limbs::ActorRig;
use crate::locomotion::Locomotion;
use crate::residents::Residents;
use bevy::prelude::*;
use fnv_actor::idle::meshes_path;
use fnv_actor::idle_play::{IdleKeys, IdlePlay};
use fnv_game::game::Game;
use fnv_game::idles::{IdlePicker, IdleRequest, IdleSource, Loops};
use fnv_game::package::flags;
use fnv_sim::FormId;
use nvf_assets::actor::{bind_clip, ActorCtx, ActorParts};
use nvf_assets::anim_play::{BlendPlayer, BoundClip, Clip, Layer};
use nvf_assets::model::ModelCtx;
use nvf_play::player::Player;
use nvf_play::script::ScriptState;
use nvf_play::RefVisual;
use std::collections::HashMap;
use std::sync::Arc;

/// The blend layers of an actor's idles: the one playing, then the one before it.
pub const IDLE_LAYER: usize = 4;

/// The first-person skeleton's camera node [observed: animation.md §9.5]. A clip with a
/// track for it is a first-person idle (idles.md §4).
pub const CAMERA_NODE: &str = "Camera1st";

/// After a collection's idle could not be started (its actor is not drawn, its clip does
/// not load), seconds until the collection is tried again [engineering choice].
const RETRY_SECONDS: f32 = 5.0;

/// One idle playing, with the clip as its rig takes it.
pub struct Playing<C> {
    /// The IDLE record (0 for a group).
    pub idle: u32,
    /// The animation group a script asked for with `PlayGroup` (`actor_groups.rs`)
    /// instead of an idle.
    pub group: Option<u16>,
    pub source: IdleSource,
    /// What asked for a package's or a marker's idle: the actor's package at the time,
    /// or the marker's base.
    owner: Option<FormId>,
    pub clip: C,
    pub play: IdlePlay,
    /// Tells this idle from the others the slot played.
    pub serial: u32,
    /// Started in the step under way.
    fresh: bool,
}

/// The idles of one rig: the one playing and, while that fades in, the one before it.
pub struct IdleSlot<C> {
    pub cur: Option<Playing<C>>,
    pub prev: Option<Playing<C>>,
    serial: u32,
}

impl<C> Default for IdleSlot<C> {
    fn default() -> Self {
        IdleSlot { cur: None, prev: None, serial: 0 }
    }
}

impl<C> IdleSlot<C> {
    /// Play an idle from its start. What played until now is held under it while it
    /// fades in; an idle started in the same step is replaced outright (a package's
    /// change idle followed by the next package's begin idle).
    fn start(&mut self, idle: u32, source: IdleSource, owner: Option<FormId>, clip: C, play: IdlePlay) {
        if let Some(old) = self.cur.take().filter(|o| !o.fresh && !o.play.gone()) {
            self.prev = Some(old);
        }
        self.serial = self.serial.wrapping_add(1);
        self.cur = Some(Playing { idle, group: None, source, owner, clip, play, serial: self.serial, fresh: true });
    }

    /// Play a script's animation group as an idle is played (`actor_groups.rs`); returns
    /// its serial.
    pub fn start_group(&mut self, group: u16, clip: C, play: IdlePlay) -> u32 {
        self.start(0, IdleSource::Script, None, clip, play);
        if let Some(c) = self.cur.as_mut() {
            c.group = Some(group);
        }
        self.serial
    }

    /// Advance by `dt` seconds.
    fn advance(&mut self, dt: f32) {
        if let Some(c) = self.cur.as_mut() {
            c.play.advance(dt);
        }
        if self.cur.as_ref().is_some_and(|c| c.play.gone()) {
            self.cur = None;
        }
        // The one before goes on as it was (a loop keeps looping, an ended one holds its
        // last pose) until the new one covers it.
        if let Some(p) = self.prev.as_mut().filter(|p| !p.play.ended()) {
            p.play.advance(dt);
        }
        if self.cur.as_ref().is_none_or(|c| c.play.ended() || c.play.weight() >= 1.0) {
            self.prev = None;
        }
    }

    /// The idle playing (not one that has ended and fades out).
    pub fn playing(&self) -> Option<&Playing<C>> {
        self.cur.as_ref().filter(|c| !c.play.ended())
    }

    pub fn is_empty(&self) -> bool {
        self.cur.is_none() && self.prev.is_none()
    }

    /// What shows, with its weight: the idle playing (fading in from its start, out
    /// from its end), and the one before it with the rest while the new one fades in.
    pub fn shown(&self) -> impl Iterator<Item = (&Playing<C>, f32)> {
        let w = self.cur.as_ref().map_or(0.0, |c| c.play.weight());
        self.cur.iter().map(move |c| (c, w)).chain(self.prev.iter().map(move |p| (p, 1.0 - w)))
    }

    /// Whether the idles cover the rig's other clips: one plays in full, or fades in
    /// over the one before it. (While the first one fades in, the other clips stay
    /// under it.)
    pub fn covers(&self) -> bool {
        self.playing().is_some_and(|c| self.prev.is_some() || c.play.weight() >= 1.0)
    }

    /// A package's or marker's idle that loops for ever leaves its loop once `owner`
    /// (the actor's package now, or the marker it stands at) is not what asked for it.
    fn check_owner(&mut self, owner_now: impl Fn(IdleSource) -> Option<FormId>) {
        // A new step: what plays was not started in it.
        if let Some(c) = self.cur.as_mut() {
            c.fresh = false;
        }
        if let Some(c) = self.cur.as_mut().filter(|c| c.source.of_package() && c.play.endless()) {
            if owner_now(c.source) != c.owner {
                c.play.leave_loop();
            }
        }
    }
}

/// The idles an actor plays (on its root, beside its [`Locomotion`]).
#[derive(Component, Default)]
pub struct ActorIdle(pub IdleSlot<Arc<BoundClip>>);

impl ActorIdle {
    /// The idles as blend layers ([`IDLE_LAYER`] and the next).
    pub fn layers(&self, player: &mut BlendPlayer) {
        for (k, (p, w)) in self.0.shown().enumerate() {
            let len = p.play.keys.length.max(1e-3);
            player.layers[IDLE_LAYER + k] = (w > 0.0).then(|| Layer { clip: p.clip.clone(), phase: (p.play.time() / len).min(0.999), weight: w });
        }
    }
}

/// The idles the player plays: on the first-person rig (clips that drive its camera
/// node) and on the third-person body (all others).
#[derive(Resource, Default)]
pub struct PlayerIdles {
    pub first: IdleSlot<Arc<Clip>>,
    pub third: IdleSlot<Arc<Clip>>,
    /// The player when an idle began to turn the view ([`Held`]).
    pub held: Option<Held>,
}

/// The player as an idle that turns the view found them. While such idles play, the
/// first-person rig stands at `feet` with that heading, the character is held there
/// (`nvf_play::furniture::PlayerSeat`), and only the view's own turning since then is
/// added to what the animation gives.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Held {
    /// Where the game last put the player if they have not moved by themselves since
    /// (`Player::placed`), else where they stand [observed: the opening's start marker
    /// is in Doc Mitchell's bed, and the collision pushes the character about 10 units
    /// off it before the wake-up starts; idles.md §4].
    pub feet: [f32; 3],
    pub heading: f32,
    pub pitch: f32,
}

/// An actor's place in an idle collection.
struct Collection {
    /// A marker's (true) or a package's, and its form.
    key: (bool, u32),
    picker: IdlePicker,
    /// The idle picked and the seconds still to wait before it.
    next: Option<(FormId, f32)>,
    /// Seconds until the collection is tried again after a failed start.
    retry: f32,
}

/// Counts for the run summary.
#[derive(Debug, Default, Clone, Copy)]
pub struct Stats {
    pub asked: u32,
    pub script: u32,
    pub package: u32,
    pub collection: u32,
    pub dialogue: u32,
    pub started: u32,
    pub player_started: u32,
    pub not_drawn: u32,
    pub no_clip: u32,
}

/// A small deterministic generator (SplitMix64) for loop counts and random picks: the
/// script runtime's random numbers are not touched, and runs repeat.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u32) -> u32 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        (((z ^ (z >> 31)) >> 32) as u32) % n.max(1)
    }
}

#[derive(Resource)]
pub struct Idles {
    collections: HashMap<u32, Collection>,
    /// What each actor was last reported to the rules as playing.
    told: HashMap<u32, u32>,
    rng: Rng,
    pub stats: Stats,
}

impl Default for Idles {
    fn default() -> Self {
        Idles { collections: HashMap::new(), told: HashMap::new(), rng: Rng(0x1D1E_5EED), stats: Stats::default() }
    }
}

/// The clip of an idle and its playback, as its record and text keys say.
fn load(ctx: &mut ActorCtx, game: &Game, idle: FormId, rng: &mut Rng) -> Result<(Arc<Clip>, IdlePlay), String> {
    let def = game.data.idles.get(&idle).ok_or("not an idle animation")?;
    let path = meshes_path(def.model.as_deref().ok_or("it names no file")?);
    let clip = ctx.clip(&path).ok_or_else(|| format!("{path} does not load"))?;
    let keys = IdleKeys::read(clip.start, clip.stop, &clip.text_keys);
    let passes = match def.loops(|n| rng.below(n)) {
        Loops::Forever => None,
        Loops::Times(n) => Some(n),
    };
    Ok((clip, IdlePlay::new(keys, passes)))
}

fn describe(game: &Game, idle: FormId, play: &IdlePlay) -> String {
    let k = play.keys;
    let loops = if play.endless() { ", looping until replaced" } else { "" };
    let range = k.loop_range.map_or(String::new(), |(a, b)| format!(", loop {a:.2}-{b:.2} s"));
    format!("{} ({:.2} s{range}{loops}, fades {:.2} / {:.2} s)", game.data.name(idle), k.length, k.blend_in, k.blend_out)
}

/// What [`drive`] reads and writes of a drawn actor.
type Drawn = (Entity, &'static RefVisual, &'static ActorRig, Option<&'static mut ActorIdle>, Option<&'static Locomotion>);

/// Take the idles asked for, run the collections, start and advance what plays.
#[allow(clippy::too_many_arguments)]
pub fn drive(
    mut commands: Commands,
    time: Res<Time>,
    mut st: ResMut<Idles>,
    mut player_idles: ResMut<PlayerIdles>,
    mut script: Option<ResMut<ScriptState>>,
    mut residents: Option<ResMut<Residents>>,
    mut sim: Option<ResMut<nvf_play::sim::SimState>>,
    player: Option<Res<Player>>,
    mut parts: ResMut<ActorParts>,
    mut models: ModelCtx,
    mut actors: Query<Drawn>,
) {
    let dt = time.delta_secs().min(0.1);
    let st = &mut *st;
    let pi = &mut *player_idles;
    let standing: HashMap<u32, Option<FormId>> = residents.as_deref().map(|r| r.standing().map(|s| (s.actor, s.marker)).collect()).unwrap_or_default();
    // The game state: the game being played, else the residents' own.
    let (game, played) = match script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
        Some(rt) => (Some(&mut rt.game), true),
        None => (residents.as_deref_mut().and_then(|r| r.own_game()), false),
    };
    let Some(game) = game else { return };
    // Between frames the game being played keeps its world in the simulation
    // (`nvf_play::script::with_world`); conditions are evaluated with it put back.
    let mut with_world = |game: &mut Game<'static>, f: &mut dyn FnMut(&mut Game<'static>)| match sim.as_deref_mut().filter(|_| played) {
        Some(sim) => {
            std::mem::swap(&mut game.world, &mut sim.world);
            f(game);
            std::mem::swap(&mut game.world, &mut sim.world);
        }
        None => f(game),
    };
    let player_ref = FormId::PLAYER_REF.0;
    let package_of = |game: &Game, actor: u32| game.extra.get(&FormId(actor)).and_then(|e| e.current_package);

    // Idles that belong to a package or a marker that is no longer the actor's.
    let owner_now = |game: &Game, actor: u32, source: IdleSource| match source {
        IdleSource::MarkerIdles => standing.get(&actor).copied().flatten(),
        _ => package_of(game, actor),
    };
    for slot in [&mut pi.first, &mut pi.third] {
        slot.check_owner(|s| owner_now(game, player_ref, s));
    }
    let mut busy: HashMap<u32, bool> = HashMap::new();
    for (_, v, _, idle, lo) in &mut actors {
        let Some(mut idle) = idle else { continue };
        idle.0.check_owner(|s| owner_now(game, v.0, s));
        // A collection's idle ends when its actor walks off [design].
        let walking = lo.is_some_and(|l| l.velocity != [0.0; 2]) || !standing.contains_key(&v.0);
        if let Some(c) = idle.0.cur.as_mut().filter(|c| walking && matches!(c.source, IdleSource::PackageIdles | IdleSource::MarkerIdles)) {
            c.play.stop();
        }
        busy.insert(v.0, idle.0.playing().is_some());
    }
    busy.insert(player_ref, pi.first.playing().is_some() || pi.third.playing().is_some());
    // An idle asked for this frame (a package's begin idle) comes before its collection.
    for r in &game.idle_requests {
        busy.insert(r.actor.0, true);
    }

    // Collections: the standing residents', and the player's under a script package.
    let candidates = standing.iter().map(|(a, m)| (*a, *m)).chain(package_of(game, player_ref).map(|_| (player_ref, None)));
    let mut live: Vec<u32> = Vec::new();
    let mut ask: Vec<(u32, FormId, IdleSource)> = Vec::new();
    for (actor, marker) in candidates {
        let data = game.data;
        let of_marker = marker.and_then(|m| data.idle_markers.get(&m).filter(|c| !c.idles.is_empty()).map(|c| ((true, m.0), c)));
        let quiet = package_of(game, actor).and_then(|p| data.package_defs.get(&p)).is_some_and(|d| d.has(flags::NO_IDLE_ANIMS));
        let Some((key, collection)) = of_marker.or_else(|| game.package_idles(FormId(actor)).map(|(p, c)| ((false, p.0), c))).filter(|_| !quiet) else { continue };
        live.push(actor);
        let c = st.collections.entry(actor).or_insert_with(|| Collection { key, picker: IdlePicker::default(), next: None, retry: 0.0 });
        if c.key != key {
            *c = Collection { key, picker: IdlePicker::default(), next: None, retry: 0.0 };
        }
        c.retry = (c.retry - dt).max(0.0);
        if busy.get(&actor).copied().unwrap_or(false) || c.retry > 0.0 {
            continue;
        }
        if c.next.is_none() {
            // The next idle of the collection whose own conditions the actor meets; when
            // none of them does, the collection is tried again later.
            let rng = &mut st.rng;
            with_world(game, &mut |game| {
                for _ in 0..collection.idles.len().max(1) {
                    c.next = c.picker.next(collection, |n| rng.below(n as u32) as usize).filter(|(idle, _)| game.idle_allowed(FormId(actor), *idle));
                    if c.next.is_some() {
                        break;
                    }
                }
            });
            if c.next.is_none() {
                c.retry = RETRY_SECONDS.max(collection.timer);
            }
        }
        let Some((idle, wait)) = c.next.as_mut() else { continue };
        *wait -= dt;
        if *wait <= 0.0 {
            ask.push((actor, *idle, if key.0 { IdleSource::MarkerIdles } else { IdleSource::PackageIdles }));
            c.next = None;
        }
    }
    st.collections.retain(|a, _| live.contains(a));
    for (actor, idle, source) in ask {
        game.ask_idle(FormId(actor), idle, source);
    }

    // Start what was asked for.
    let requests = std::mem::take(&mut game.idle_requests);
    if !requests.is_empty() {
        let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
        let drawn: HashMap<u32, Entity> = actors.iter().map(|(e, v, ..)| (v.0, e)).collect();
        for IdleRequest { actor, idle, source } in requests {
            st.stats.asked += 1;
            match source {
                IdleSource::Script => st.stats.script += 1,
                IdleSource::PackageBegin | IdleSource::PackageEnd | IdleSource::PackageChange => st.stats.package += 1,
                IdleSource::PackageIdles | IdleSource::MarkerIdles => st.stats.collection += 1,
                IdleSource::Speaker | IdleSource::Listener => st.stats.dialogue += 1,
            }
            let name = game.data.name(actor);
            let failed = |st: &mut Idles, why: &str| {
                if let Some(c) = st.collections.get_mut(&actor.0).filter(|_| matches!(source, IdleSource::PackageIdles | IdleSource::MarkerIdles)) {
                    c.retry = RETRY_SECONDS;
                }
                debug!("idle: {name} does not play {} ({source:?}): {why}", game.data.name(idle));
            };
            if actor.0 != player_ref && !drawn.contains_key(&actor.0) {
                st.stats.not_drawn += 1;
                failed(st, "the actor is not drawn");
                continue;
            }
            let (clip, play) = match load(&mut ctx, game, idle, &mut st.rng) {
                Ok(l) => l,
                Err(why) => {
                    st.stats.no_clip += 1;
                    warn!("idle: {name} cannot play {} ({source:?}): {why}", game.data.name(idle));
                    failed(st, &why);
                    continue;
                }
            };
            let owner = owner_now(game, actor.0, source).filter(|_| source.of_package());
            let what = describe(game, idle, &play);
            if actor.0 == player_ref {
                let first = clip.tracks.iter().any(|(n, _)| n == CAMERA_NODE);
                if first && pi.held.is_none() {
                    pi.held = player.as_deref().map(|p| Held { feet: p.placed.unwrap_or(p.feet), heading: p.heading, pitch: p.pitch });
                }
                if first { &mut pi.first } else { &mut pi.third }.start(idle.0, source, owner, clip, play);
                st.stats.player_started += 1;
                let facing = player.as_deref().map_or(String::new(), |p| {
                    // A first-person idle plays where the player is held.
                    let at = pi.held.filter(|_| first).map_or(p.feet, |h| h.feet);
                    format!(", at ({:.1}, {:.1}, {:.1}) facing {:.0} degrees, pitch {:.0}", at[0], at[1], at[2], p.heading.to_degrees(), p.pitch.to_degrees())
                });
                info!("idle: the player plays {what} on the {} rig ({source:?}){facing}", if first { "first-person" } else { "third-person" });
                continue;
            }
            let Ok((e, _, rig, slot, _)) = actors.get_mut(drawn[&actor.0]) else { continue };
            let bound = bind_clip(&rig.skel, clip);
            if bound.tracks.is_empty() {
                st.stats.no_clip += 1;
                failed(st, "its clip drives no node of the actor's skeleton");
                continue;
            }
            let bound = Arc::new(bound);
            match slot {
                Some(mut s) => s.0.start(idle.0, source, owner, bound, play),
                None => {
                    let mut s = ActorIdle::default();
                    s.0.start(idle.0, source, owner, bound, play);
                    commands.entity(e).insert(s);
                }
            }
            st.stats.started += 1;
            match source {
                IdleSource::PackageIdles | IdleSource::MarkerIdles => debug!("idle: {name} plays {what} ({source:?})"),
                _ => info!("idle: {name} plays {what} ({source:?})"),
            }
        }
    }

    // Advance, and tell the rules what plays.
    let mut now: HashMap<u32, u32> = HashMap::new();
    for (e, v, _, idle, _) in &mut actors {
        let Some(mut idle) = idle else { continue };
        idle.0.advance(dt);
        match idle.0.playing() {
            // A script's animation group is not an idle.
            Some(p) if p.group.is_some() => {}
            Some(p) => {
                now.insert(v.0, p.idle);
            }
            None if idle.0.is_empty() => {
                commands.entity(e).remove::<ActorIdle>();
            }
            None => {}
        }
    }
    for slot in [&mut pi.first, &mut pi.third] {
        slot.advance(dt);
    }
    if let Some(p) = pi.first.playing().or(pi.third.playing()) {
        now.insert(player_ref, p.idle);
    }
    for (actor, idle) in &now {
        if st.told.get(actor) != Some(idle) {
            game.set_idle_playing(FormId(*actor), Some(FormId(*idle)));
        }
    }
    for actor in st.told.keys().filter(|a| !now.contains_key(a)) {
        game.set_idle_playing(FormId(*actor), None);
    }
    st.told = now;
}

/// The idle line of the run summary.
pub fn report(mut exit: MessageReader<AppExit>, st: Res<Idles>) {
    if exit.read().next().is_none() {
        return;
    }
    let s = st.stats;
    info!(
        "idle summary: {} idles asked for ({} by scripts, {} by packages' begin, end and change, {} from collections, {} by dialogue lines); {} started on actors, {} on the player; {} for actors not drawn, {} without a usable clip",
        s.asked, s.script, s.package, s.collection, s.dialogue, s.started, s.player_started, s.not_drawn, s.no_clip
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn play(length: f32, blend_in: f32, passes: Option<u32>) -> IdlePlay {
        IdlePlay::new(IdleKeys { length, loop_range: None, blend_in, blend_out: 0.25 }, passes)
    }

    fn weights(s: &IdleSlot<u8>) -> Vec<(u8, f32)> {
        s.shown().map(|(p, w)| (p.clip, w)).collect()
    }

    #[test]
    fn a_new_idle_fades_in_over_the_one_before() {
        let mut s: IdleSlot<u8> = IdleSlot::default();
        s.start(1, IdleSource::Script, None, 1, play(2.0, 0.0, Some(1)));
        s.advance(0.5);
        assert_eq!(weights(&s), [(1, 1.0)]);
        assert!(s.covers());
        s.check_owner(|_| None);
        s.start(2, IdleSource::Script, None, 2, play(4.0, 1.0, None));
        s.advance(0.25);
        // Together they cover the rig in full.
        assert_eq!(weights(&s), [(2, 0.25), (1, 0.75)]);
        assert!(s.covers() && s.playing().unwrap().idle == 2);
        s.advance(0.75);
        assert_eq!(weights(&s), [(2, 1.0)]);
    }

    #[test]
    fn an_idle_started_in_the_same_step_is_replaced_outright() {
        let mut s: IdleSlot<u8> = IdleSlot::default();
        s.start(1, IdleSource::PackageIdles, None, 1, play(2.0, 0.0, None));
        s.advance(1.0);
        s.check_owner(|_| None);
        // A change idle, then the next package's begin idle, in one frame.
        s.start(2, IdleSource::PackageChange, None, 2, play(2.0, 0.5, None));
        s.start(3, IdleSource::PackageBegin, None, 3, play(2.0, 0.5, Some(1)));
        s.advance(0.25);
        assert_eq!(weights(&s), [(3, 0.5), (1, 0.5)]);
    }

    #[test]
    fn a_one_shot_idle_ends_fades_and_goes() {
        let mut s: IdleSlot<u8> = IdleSlot::default();
        s.start(1, IdleSource::Speaker, None, 1, play(1.0, 0.5, Some(1)));
        s.advance(0.25);
        // Fading in alone: the rig's other clips stay under it.
        assert!(!s.covers() && s.playing().is_some());
        s.advance(0.75);
        assert!(s.playing().is_none() && !s.is_empty());
        s.advance(0.125);
        assert_eq!(weights(&s), [(1, 0.5)]);
        s.advance(0.25);
        assert!(s.is_empty());
    }

    #[test]
    fn a_package_loop_is_left_when_the_package_is_no_longer_the_actors() {
        let (p, q) = (Some(FormId(1)), Some(FormId(2)));
        let mut s: IdleSlot<u8> = IdleSlot::default();
        s.start(1, IdleSource::PackageBegin, p, 1, play(1.0, 0.0, None));
        s.advance(0.5);
        s.check_owner(|_| p);
        assert!(s.playing().unwrap().play.endless());
        s.check_owner(|_| q);
        assert!(!s.playing().unwrap().play.endless());
        s.advance(0.75);
        assert!(s.playing().is_none());
        // A script's idle is nobody's package's.
        let mut s: IdleSlot<u8> = IdleSlot::default();
        s.start(1, IdleSource::Script, None, 1, play(1.0, 0.0, None));
        s.check_owner(|_| q);
        assert!(s.playing().unwrap().play.endless());
    }

    #[test]
    fn rolls_stay_below_the_bound() {
        let mut r = Rng(7);
        assert!((0..200).all(|_| r.below(5) < 5));
        assert_eq!(r.below(0), 0);
    }
}
