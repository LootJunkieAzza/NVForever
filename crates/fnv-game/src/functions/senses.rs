//! What the engine sees of actors and tells the scripts: movement, sneaking, line of
//! sight, the camera, and who always shows subtitles (docs/design/script-stubs.md §3).
//!
//! The engine measures; this module only keeps the latest word ([`Senses`]) and answers
//! the script functions from it. An actor the engine has not told about (not drawn, too
//! far from the player, a headless run) reads as standing still, not sneaking and not
//! seen [engineering choice: the same "not measured is no" rule as GetDetected].

use super::{none, yes, Args, Ctx};
use crate::game::Game;
use fnv_script::vm::Value;
use fnv_sim::FormId;
use std::collections::{BTreeMap, BTreeSet};

/// One actor's movement as the engine last measured it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Motion {
    /// It moves (above the engine's walking threshold).
    pub moving: bool,
    /// It moves at its run speed (the player: not holding Walk).
    pub running: bool,
    /// It is sneaking (crouched).
    pub sneaking: bool,
}

/// What the engine told the scripts, and the switches scripts set for it.
#[derive(Debug, Clone, PartialEq)]
pub struct Senses {
    pub motion: BTreeMap<FormId, Motion>,
    /// The camera shows the player from the eyes (first person). The game starts so
    /// [observed: the PC version opens in first person; `nvf_play` `Pov::First`].
    pub first_person: bool,
    /// Actors whose lines always show their subtitle, heard or not
    /// (AlwaysShowActorSubtitles 1).
    pub always_subtitles: BTreeSet<FormId>,
    /// Look: the reference each actor's head is held on, until StopLook. Not part of the
    /// saved state yet.
    pub looks: BTreeMap<FormId, Look>,
}

impl Default for Senses {
    fn default() -> Senses {
        Senses { motion: BTreeMap::new(), first_person: true, always_subtitles: BTreeSet::new(), looks: BTreeMap::new() }
    }
}

/// A script's Look on one actor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    pub target: FormId,
    /// The whole body turns to the target, not only the head (Look's second parameter).
    pub rotate_body: bool,
}

impl Game<'_> {
    /// The engine tells how an actor moves (the player included).
    pub fn set_motion(&mut self, actor: FormId, m: Motion) {
        if self.senses.motion.get(&actor) != Some(&m) {
            self.senses.motion.insert(actor, m);
        }
    }

    /// The actor left the engine's view (unloaded, disabled, dead): forget its motion.
    pub fn forget_motion(&mut self, actor: FormId) {
        self.senses.motion.remove(&actor);
    }

    /// The engine tells whether the camera is in first person.
    pub fn set_first_person(&mut self, on: bool) {
        self.senses.first_person = on;
    }

    pub fn motion(&self, actor: FormId) -> Motion {
        self.senses.motion.get(&actor).copied().unwrap_or_default()
    }

    /// Whether the actor's lines show their subtitle out of hearing too.
    pub fn always_subtitles(&self, actor: FormId) -> bool {
        self.senses.always_subtitles.contains(&actor)
    }

    /// The reference a script holds the actor's head on (Look), if any.
    pub fn script_look(&self, actor: FormId) -> Option<Look> {
        self.senses.looks.get(&actor).copied()
    }
}

/// `actor.IsSneaking`: 1 while the actor is sneaking (crouched) [documented: GECK wiki
/// IsSneaking]. The engine tells it ([`Game::set_motion`]); actors other than the player
/// never sneak yet (packages' Always Sneak is not acted on).
pub fn is_sneaking(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject_or_player();
    yes(g.motion(r).sneaking)
}

/// `actor.IsMoving`: 0 not moving, 1 forward, 2 backward, 3 left, 4 right: the
/// direction the actor is asking to move in [documented: GECK wiki IsMoving]. The engine
/// tells only whether it moves, so a moving actor reads 1, forward [hypothesis: actors
/// walk where they face; the player's strafing and backing up are not told apart yet].
pub fn is_moving(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject_or_player();
    Ok(Value::Num(if g.motion(r).moving { 1.0 } else { 0.0 }))
}

/// `actor.IsRunning`: 1 when the actor's speed mode is run rather than walk
/// [documented: catalogue summary]. Read from the engine's measure: running while it
/// moves at its run speed [hypothesis: an actor standing still in run mode reads 0].
pub fn is_running(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject_or_player();
    yes(g.motion(r).running)
}

/// `actor.GetLineOfSight target`: 1 when the calling actor can see the target: nothing
/// solid between them and within ±95° of where it looks [documented: GECK wiki
/// GetLineOfSight]. Read from the detection the engine last measured for the pair
/// (`crate::detection`, `Detection::seen`: line of sight and field of view): pairs
/// within detection reach of the player. A pair not measured reads 0, as does the
/// player as the caller (the player is not a detector) [engineering choice]. A target
/// that is not an actor is not measured: 0 [documented: on a non-player actor GetLOS
/// returns false for anything but an actor].
pub fn get_line_of_sight(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let target = a.form(0)?;
    yes(g.detection.get(r, target).is_some_and(|d| d.seen))
}

/// `IsPC1stPerson`: 1 while the camera is in first person [documented: GECK wiki
/// IsPC1stPerson]; the engine tells it ([`Game::set_first_person`]).
pub fn is_pc_1st_person(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    yes(g.senses.first_person)
}

/// `actor.AlwaysShowActorSubtitles toggle`: 1 shows the actor's subtitles always, 0
/// back to normal [documented: catalogue summary]. "Always" is taken to mean also out
/// of hearing, where a line otherwise shows none (`nvf_play::voice::audible`)
/// [hypothesis: the original's "normal" also hides subtitles by distance].
pub fn always_show_actor_subtitles(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    if a.num_or(0, 0.0) != 0.0 {
        g.senses.always_subtitles.insert(r);
    } else {
        g.senses.always_subtitles.remove(&r);
    }
    none()
}
