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
    /// It is turning: 0 not, 1 left, 2 right (IsTurning's codes).
    pub turning: u8,
    /// Its speed mode is run, whether or not it moves (the player: Walk not toggled).
    pub run_mode: bool,
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
    /// When ([`Game::time`]) the animating side last told whether each actor has its
    /// weapon out (`Game::set_weapon_out`): it tells every frame while it draws the actor.
    pub weapon_told: BTreeMap<FormId, f64>,
    /// The open state of every object the engine animates open and closed (doors,
    /// animated containers), as GetOpenState numbers it: 1 open, 2 opening, 3 closed,
    /// 4 closing. Told by the engine's door animation (`nvf-play` doors.rs).
    pub open_states: BTreeMap<FormId, u8>,
    /// The special animation action each drawn actor is doing, as GetAnimAction numbers
    /// it (0 equipping a weapon, 1 unequipping), told with its weapon state
    /// (`Game::set_anim_action`); absent: none.
    pub anim_actions: BTreeMap<FormId, i32>,
}

impl Default for Senses {
    fn default() -> Senses {
        Senses { motion: BTreeMap::new(), first_person: true, always_subtitles: BTreeSet::new(), looks: BTreeMap::new(), weapon_told: BTreeMap::new(), open_states: BTreeMap::new(), anim_actions: BTreeMap::new() }
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

    /// The actors a script's Look turns whole (`rotateBody` 1), each with its target, in
    /// reference order: the body faces the target until StopLook (`fnv_ai` turns those
    /// it runs, the engine's `script_look.rs` the others).
    pub fn body_looks(&self) -> Vec<(FormId, FormId)> {
        self.senses.looks.iter().filter(|(_, l)| l.rotate_body).map(|(a, l)| (*a, l.target)).collect()
    }
}

/// `actor.IsSneaking`: 1 while the actor is sneaking (crouched) [documented: GECK wiki
/// IsSneaking]. The engine tells it ([`Game::set_motion`]): the player's sneak control,
/// and for other actors [`Game::sneaks`] (SetForceSneak, a package's Always Sneak).
/// An actor the engine does not measure (not drawn, far) sneaks by the same rule
/// ([`Game::sneaks`]).
pub fn is_sneaking(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject_or_player();
    yes(g.senses.motion.get(&r).map_or_else(|| g.sneaks(r), |m| m.sneaking))
}

/// `actor.IsMoving`: 0 not moving, 1 forward, 2 backward, 3 left, 4 right: the
/// direction the actor is asking to move in [documented: GECK wiki IsMoving]. The engine
/// tells only whether it moves, so a moving actor reads 1, forward [hypothesis: actors
/// walk where they face; the player's strafing and backing up are not told apart yet].
pub fn is_moving(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject_or_player();
    Ok(Value::Num(if g.motion(r).moving { 1.0 } else { 0.0 }))
}

/// `actor.IsRunning`: 1 when the actor's speed mode is run rather than walk, "even if
/// they are standing still" [documented: GECK wiki IsRunning notes; observed: the player
/// standing still read 1 and Sunny Smiles standing still 0, capture `fa_batch1_queries`].
/// The player's mode is the Walk toggle (`Motion::run_mode`); another actor's is read
/// from the engine's measure, running while it moves at its run speed [hypothesis for
/// NPCs: their mode while standing is not told].
pub fn is_running(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject_or_player();
    let m = g.motion(r);
    yes(m.running || m.run_mode)
}

/// `actor.GetLineOfSight target`: 1 when the calling actor can see the target: nothing
/// solid between them and within ±95° of where it looks [documented: GECK wiki
/// GetLineOfSight]. Read from the detection the engine last measured for the pair
/// (`crate::detection`, `Detection::seen`: line of sight and field of view): every two
/// actors of the loaded area within detection reach of each other (`crate::sight`). On
/// the player it is the camera that sees, from where the camera is and along its view
/// [documented: same page, "it returns true if the *camera* can see the object";
/// observed: player.GetLineOfSight Ringo 1, function probes fp1, 2026-10-17]. A pair not
/// measured reads 0 [engineering choice]. A target that is not an actor is not
/// measured: 0 [documented: on a non-player actor GetLOS returns false for anything but
/// an actor; on the player the original also tests other objects, which NVForever does
/// not yet].
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

/// `actor.IsTurning`: 0 not turning, 1 turning left, 2 turning right [documented: GECK
/// wiki IsTurning]. The engine tells it with the rest of the movement
/// ([`Game::set_motion`]; `nvforever` `detection.rs` measures the heading's change).
pub fn is_turning(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject_or_player();
    Ok(Value::Num(g.motion(r).turning.min(2) as f64))
}

/// `IsPlayerMovingIntoNewSpace`: 1 while the player is moving into a new space, through a
/// load door or by fast travel [documented: GECK wiki IsPlayerMovingIntoNewSpace]: while
/// the engine holds the game under a loading screen ([`Game::loading`], set by
/// `nvf_play::script` from the travel's frozen frames). Scripts do not run then, so a
/// script reads 0 in its own blocks, as the page's note warns [documented: "may only
/// work within a MenuMode, OnLoad or similar block"]; package conditions evaluated during
/// the load see 1.
pub fn is_player_moving_into_new_space(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    yes(g.loading)
}

/// `actor.SetForceSneak flag`: forced sneaking on (non-zero) or off [documented: GECK
/// wiki SetForceSneak]. The engine crouches the actor and walks it at the sneaking pace
/// while it holds ([`Game::sneaks`]).
pub fn set_force_sneak(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    g.extra.entry(r).or_default().force_sneak = a.num_or(0, 0.0) != 0.0;
    none()
}

impl Game<'_> {
    /// Whether `actor`'s current package (as last chosen) has the PKDT general flag `flag`
    /// (`crate::package::flags`).
    pub fn package_has(&self, actor: FormId, flag: u32) -> bool {
        let data = self.data;
        self.extra.get(&actor).and_then(|e| e.current_package).and_then(|p| data.package_defs.get(&p)).is_some_and(|d| d.has(flag))
    }

    /// Whether an actor other than the player sneaks: SetForceSneak holds, or its current
    /// package has "Always Sneak" ("Actor always sneaks during this package"
    /// [documented: GECK wiki Packages, Flags tab]). In a combat state an actor stands up
    /// to fight unless its package goes on during combat ("Continue During Combat")
    /// [hypothesis].
    pub fn sneaks(&self, actor: FormId) -> bool {
        if actor == FormId::PLAYER_REF || self.extra.get(&actor).is_some_and(|e| e.dead) {
            return false;
        }
        if self.extra.get(&actor).is_some_and(|e| e.force_sneak) {
            return true;
        }
        // Crouched to reload in a weapon use (`crate::use_weapon`).
        if self.weapon_uses.get(&actor).is_some_and(|w| w.crouched) {
            return true;
        }
        use crate::package::flags;
        let fighting = self.awareness.state(actor) == crate::awareness::State::Combat;
        // Crouched to fire by the combat AI (docs/design/combat-ai.md §14.2).
        if fighting && self.awareness.crouching.contains(&actor) {
            return true;
        }
        // A person hiding at its flee spot, in any combat state (§12.2.1).
        if self.at_flee_spot(actor) && self.world.placed.get(actor).is_some_and(|p| &p.sig == b"ACHR") {
            return true;
        }
        self.package_has(actor, flags::ALWAYS_SNEAK) && (!fighting || self.package_has(actor, flags::CONTINUE_DURING_COMBAT))
    }
}
