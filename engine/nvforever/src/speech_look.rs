//! Turning to whom a line is said (docs/design/dialogue-play.md §9): while an actor says
//! a line to someone outside a conversation with the player (a script's SayTo, a hello
//! or a Goodbye greeting at the player, a line of a conversation between two actors),
//! the speaker turns its head and body to the listener and the listener looks back.
//! Who turns to whom is the game's (`fnv_game::game::Game::facings`, which reads
//! SayTo's `noTargetLookFlag` and StartConversation's `headTrack`); this module turns
//! the drawn actors.
//!
//! - **Heads.** Each actor that turns is given a forced head-tracking target
//!   ([`LookOverride`], `head_look.rs`): the listener's eyes, or the player's. Head
//!   tracking turns the head within its usual limits and eases it back when the
//!   override is taken off at the end of the line. An override something else set (a
//!   test aid) is left alone; so is the actor of a conversation with the player
//!   (`dialogue_look.rs` has its head). With `--no-head-tracking` no head turns.
//! - **Bodies.** A speaker (and, in a conversation between actors, the listener)
//!   standing in its idle whose body faces more than `BODY_TURN_FROM` away turns on the
//!   spot, once per line, as in a conversation with the player
//!   (`dialogue_look::BodyTurn`, the same speed and turn clips) [owner decision
//!   2026-10-11: "speaker turns head and body"]. Not turned here: an actor the AI runs
//!   (`--npc-packages`: `fnv_ai` turns its body, residents.rs draws it), one in
//!   furniture ([`BodyHeld`]), a `--npc-travel` traveller, one under the walk test, a
//!   walking one, a creature, a dead one. The body keeps its new heading afterwards.
//! - **Pausing.** The turns run on the virtual clock; a menu holds a SayTo line and the
//!   turns with it.
//!
//! The speeds and limits are NVForever's design choices, as in `dialogue_look.rs`.
//! `--no-dialogue-look` switches this off too.
//!
//! Log, starting `speech look:`: one line per actor turning for a line (what turns and
//! why not the body); at exit a summary line.

use crate::dialogue_look::{heading_deg, yaw_pitch, BodyHeld, BodyTurn, Looks};
use crate::head_look::{HeadLookSettings, LookAt, LookOverride};
use crate::limbs::ActorRig;
use crate::locomotion::{Dying, Locomotion, WalkTest};
use crate::nav::NavState;
use crate::ragdoll::Corpse;
use crate::residents::Residents;
use bevy::prelude::*;
use fnv_actor::locomotion::Move;
use fnv_game::game::Facing;
use fnv_sim::FormId;
use nvf_play::combat::is_dead;
use nvf_play::dialogue_look::LookSettings;
use nvf_play::pause::{MenuPause, Paused};
use nvf_play::player::Player;
use nvf_play::script::ScriptState;
use nvf_play::sim::SimState;
use nvf_play::{space, PlaySet};
use std::collections::{HashMap, HashSet};

/// The heads and bodies this module turns, and what it did, for the summary.
#[derive(Resource, Default)]
pub struct SpeechLooks {
    /// Head-tracking targets this module set, by reference.
    heads: HashMap<u32, (Entity, LookAt)>,
    /// Bodies turning on the spot, by reference.
    bodies: HashMap<u32, (Entity, BodyTurn)>,
    /// The (who, to, line) turns already begun, and the actor's entity.
    begun: HashMap<(u32, u32, u32), Entity>,
    /// The lines whose speaker was not drawn (said once).
    undrawn: HashSet<(u32, u32, u32)>,
    /// Lines that turned their speaker; heads told to turn; bodies turned here; bodies
    /// left to the AI; bodies that stayed (and why, by reason).
    lines: usize,
    head_turns: usize,
    body_turns: usize,
    by_ai: usize,
    stayed: HashMap<&'static str, usize>,
    /// Said once: head tracking is off.
    warned: bool,
}

pub struct SpeechLookPlugin;

impl Plugin for SpeechLookPlugin {
    fn build(&self, app: &mut App) {
        // After the frame's animation and the play systems (this frame's lines), like
        // a conversation's turn; head tracking runs after it (head_look.rs).
        app.init_resource::<SpeechLooks>().add_systems(Update, face.after(crate::locomotion::drive).after(PlaySet::Ui).after(crate::dialogue_look::turn_speaker)).add_systems(Last, report);
    }
}

/// What [`face`] reads of an actor.
type Actor = (Entity, &'static ActorRig, &'static ChildOf, Option<&'static LookOverride>, Has<Dying>, Has<Corpse>, Has<WalkTest>, Has<BodyHeld>);

/// The body is the AI's to turn.
const BY_AI: &str = "the AI turns it (--npc-packages)";

/// Why a body does not turn here, or `None` when it does.
#[allow(clippy::too_many_arguments)]
fn body_stays(f: &Facing, npc: bool, dead: bool, held: bool, walk_test: bool, traveller: bool, by_ai: bool, lo: Option<&Locomotion>) -> Option<&'static str> {
    if !f.body {
        Some("a listener of a SayTo line turns its head only")
    } else if dead {
        Some("it is dead")
    } else if !npc {
        Some("it is not a human-style NPC")
    } else if held {
        Some("it is in furniture")
    } else if by_ai {
        Some(BY_AI)
    } else if traveller {
        Some("its heading is its route's (--npc-travel)")
    } else if walk_test {
        Some("the walk test moves it")
    } else if !lo.is_none_or(|lo| lo.blend.cur.mv == Move::Idle && lo.velocity == [0.0; 2]) {
        Some("it is not standing in its idle")
    } else {
        None
    }
}

/// Turn the actors to whom they talk, and let go of those whose line is over.
#[allow(clippy::too_many_arguments)]
pub(crate) fn face(
    mut commands: Commands,
    time: Res<Time>,
    (settings, heads_on, mode, paused): (Res<LookSettings>, Res<HeadLookSettings>, Res<MenuPause>, Res<Paused>),
    script: Option<Res<ScriptState>>,
    sim: Res<SimState>,
    player: Option<Res<Player>>,
    nav: Option<Res<NavState>>,
    residents: Option<Res<Residents>>,
    talk: Option<Res<Looks>>,
    mut looks: ResMut<SpeechLooks>,
    actors: Query<Actor>,
    mut locos: Query<&mut Locomotion>,
    globals: Query<&GlobalTransform>,
    mut transforms: Query<&mut Transform>,
) {
    let game = script.as_ref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game);
    let in_conversation = game.and_then(|g| g.dialogue_actor()).map(|a| a.0);
    // The actor of a conversation with the player is `dialogue_look.rs`'s; the player
    // is turned by nobody.
    let facings: Vec<Facing> = match game.filter(|_| settings.enabled) {
        Some(g) => g.facings().into_iter().filter(|f| f.who != FormId::PLAYER_REF && Some(f.who.0) != in_conversation).collect(),
        None => Vec::new(),
    };
    let looks = &mut *looks;
    let by_ref: HashMap<u32, usize> = actors.iter().enumerate().map(|(k, a)| (a.1.reference, k)).collect();
    let list: Vec<_> = actors.iter().collect();
    let name = |r: FormId| if r == FormId::PLAYER_REF { "the player".to_string() } else { game.map_or_else(|| format!("{:08X}", r.0), |g| g.data.name(r)) };
    // Where `to` stands (Bevy space), and how far it is off the front of `entity`'s body
    // as drawn last frame, radians.
    let place_of = |to: FormId| {
        if to == FormId::PLAYER_REF {
            player.as_ref().map(|p| space::to_bevy(p.feet))
        } else {
            by_ref.get(&to.0).and_then(|&j| globals.get(list[j].0).ok()).map(|g| g.translation())
        }
    };
    let off_now = |entity: Entity, to: FormId| globals.get(entity).ok().zip(place_of(to)).map(|(g, t)| yaw_pitch(g.affine().inverse().transform_point3(t)).0);

    // Heads: speaking wins over listening when an actor does both.
    let mut want: HashMap<u32, LookAt> = HashMap::new();
    for f in facings.iter().filter(|f| !f.speaking).chain(facings.iter().filter(|f| f.speaking)) {
        want.insert(f.who.0, if f.to == FormId::PLAYER_REF { LookAt::Player } else { LookAt::Actor(f.to.0) });
    }
    if !want.is_empty() && !heads_on.enabled && !looks.warned {
        looks.warned = true;
        info!("speech look: head tracking is off (--no-head-tracking): no head turns to whom it talks to");
    }
    for (&r, &at) in &want {
        let Some(&k) = by_ref.get(&r) else { continue };
        let (entity, _, _, forced, ..) = list[k];
        if talk.as_ref().is_some_and(|t| t.turns_head(r)) {
            continue;
        }
        let ours = looks.heads.get(&r).filter(|(e, _)| *e == entity).map(|h| h.1);
        if forced.is_some_and(|o| Some(o.0) != ours) {
            // Something else forces this head's target.
            continue;
        }
        if ours != Some(at) {
            commands.entity(entity).insert(LookOverride(at));
            looks.heads.insert(r, (entity, at));
        }
    }
    let over: Vec<u32> = looks.heads.keys().copied().filter(|r| !want.contains_key(r)).collect();
    for r in over {
        let (entity, at) = looks.heads.remove(&r).expect("listed");
        if let Some(&k) = by_ref.get(&r) {
            let (e, _, _, forced, ..) = list[k];
            if e == entity && forced.is_some_and(|o| o.0 == at) {
                commands.entity(entity).remove::<LookOverride>();
            }
        }
    }

    // Each actor's turn for a line, once.
    let now: HashSet<(u32, u32, u32)> = facings.iter().map(|f| (f.who.0, f.to.0, f.info.0)).collect();
    let ended: Vec<((u32, u32, u32), Entity)> = looks.begun.iter().filter(|(k, _)| !now.contains(k)).map(|(k, e)| (*k, *e)).collect();
    looks.undrawn.retain(|k| now.contains(k));
    for ((who, to, info), entity) in ended {
        looks.begun.remove(&(who, to, info));
        // For the log: how far it ended up from facing the other.
        if let Some(off) = off_now(entity, FormId(to)) {
            info!("speech look: line {info:08X} over: {} has {} {:+.0}° off its front", name(FormId(who)), name(FormId(to)), off.to_degrees());
        }
    }
    for f in &facings {
        let key = (f.who.0, f.to.0, f.info.0);
        if looks.begun.contains_key(&key) {
            continue;
        }
        let Some(&k) = by_ref.get(&f.who.0) else {
            if f.speaking && looks.undrawn.insert(key) {
                info!("speech look: {} says a line to {} but is not drawn: nobody turns", name(f.who), name(f.to));
            }
            continue;
        };
        looks.begun.insert(key, list[k].0);
        if f.speaking {
            looks.lines += 1;
        }
        let (entity, _, child_of, _, dying, corpse, walk_test, held) = list[k];
        let r = f.who.0;
        looks.head_turns += usize::from(heads_on.enabled);
        let npc = sim.refs.get(f.who).is_some_and(|p| &p.sig == b"ACHR");
        let dead = dying || corpse || is_dead(script.as_deref(), r);
        let traveller = nav.as_ref().is_some_and(|n| n.travellers.iter().any(|t| t.reference == r));
        let by_ai = residents.as_ref().is_some_and(|res| res.runs(r));
        let stays = body_stays(f, npc, dead, held, walk_test, traveller, by_ai, locos.get(entity).ok());
        // Where the listener stands, in the actor's own space.
        let target = place_of(f.to);
        let root = transforms.get(entity).ok().copied();
        let parent = globals.get(child_of.parent()).ok().map(|g| g.affine());
        let off = root.zip(parent).zip(target).map(|((root, parent), t)| yaw_pitch((parent * root.compute_affine()).inverse().transform_point3(t)).0);
        let role = if f.speaking { "says a line to" } else { "listens to" };
        let off_deg = off.map_or(f32::NAN, f32::to_degrees);
        let who = name(f.who);
        let to = name(f.to);
        match stays {
            Some(why) => {
                if why == BY_AI {
                    looks.by_ai += 1;
                } else {
                    *looks.stayed.entry(why).or_default() += 1;
                }
                info!("speech look: {who} {role} {to} and turns its head ({to} is {off_deg:+.0}° off its front); the body stays here: {why}");
            }
            None => match root.zip(off).and_then(|(root, off)| BodyTurn::plan(root.rotation, off).map(|b| (root, b))) {
                Some((root, b)) => {
                    info!(
                        "speech look: {who} {role} {to} and turns head and body: {to} is {off_deg:+.0}° off its front, the body turns {:+.0}° in {:.2} s from heading {:.0}°",
                        b.total.to_degrees(),
                        b.seconds,
                        heading_deg(root.rotation)
                    );
                    looks.body_turns += 1;
                    looks.bodies.insert(r, (entity, b));
                }
                None => info!("speech look: {who} {role} {to} and turns its head ({to} is {off_deg:+.0}° off its front: the body need not turn)"),
            },
        }
    }

    // The bodies turning. With `--menu-pause original` the picture stands still while a
    // menu is up: a turn is completed at once, as in a conversation.
    let dt = time.delta_secs();
    let instant = *mode == MenuPause::Original && !paused.0 && game.is_some_and(|g| g.menu.is_some() || g.dialogue.is_some());
    let mut done = Vec::new();
    for (&r, (entity, b)) in looks.bodies.iter_mut() {
        let gone = actors.get(*entity).map_or(true, |a| a.4 || a.5) || is_dead(script.as_deref(), r) || Some(r) == in_conversation;
        if gone {
            if let Ok(mut lo) = locos.get_mut(*entity) {
                lo.turn = 0.0;
            }
            done.push(r);
            continue;
        }
        let (rot, rate) = b.step(dt, instant);
        if let Ok(mut t) = transforms.get_mut(*entity) {
            t.rotation = rot;
        }
        let finished = b.done();
        if let Ok(mut lo) = locos.get_mut(*entity) {
            lo.turn = if finished { 0.0 } else { rate };
        }
        if finished {
            done.push(r);
        }
    }
    for r in done {
        looks.bodies.remove(&r);
    }
}

/// The summary line at exit.
fn report(mut exit: MessageReader<AppExit>, looks: Res<SpeechLooks>) {
    if exit.read().next().is_none() {
        return;
    }
    let mut stayed: Vec<String> = looks.stayed.iter().map(|(why, n)| format!("{n} ({why})")).collect();
    stayed.sort();
    info!(
        "speech look summary: {} lines said to someone turned their speaker; {} heads turned, {} bodies turned on the spot, {} left to the AI, {} stayed{}",
        looks.lines,
        looks.head_turns,
        looks.body_turns,
        looks.by_ai,
        looks.stayed.values().sum::<usize>(),
        if stayed.is_empty() { String::new() } else { format!(": {}", stayed.join(", ")) }
    );
}
