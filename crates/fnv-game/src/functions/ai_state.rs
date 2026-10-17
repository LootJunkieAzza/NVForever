//! AI state functions over the package model (`crate::ai`) and the detection states
//! (`crate::awareness`): ResetAI, GetCurrentAIPackage, GetCurrentAIProcedure, and the
//! forced head tracking of Look and StopLook (docs/design/script-stubs.md §3).

use super::{none, Args, Ctx};
use crate::awareness::State;
use crate::functions::furniture::bed;
use crate::functions::senses::Look;
use crate::game::Game;
use crate::package::PackageType;
use fnv_script::vm::Value;
use fnv_sim::FormId;

/// `actor.ResetAI`: clears the actor's current AI behaviour; its packages are evaluated
/// afresh, and packages added with AddScriptPackage stop and are removed [documented:
/// GECK wiki ResetAI]. Here: the script package goes, the current package's "goal met"
/// and the packages it could not reach are forgotten, a Look is let go, and the actor
/// chooses again (`Game::evaluate_package`). Combat and pathing, which the page says
/// are re-evaluated too, are the engine's and the combat AI's: they are not touched
/// here [engineering choice: combat states belong to `crate::awareness` and the combat
/// workstream; the walker takes the new package on its next step].
pub fn reset_ai(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    if let Some(e) = g.extra.get_mut(&r) {
        e.packages.clear();
        e.package_goal_met = false;
        e.package_blocked.clear();
    }
    g.senses.looks.remove(&r);
    // A weapon use (UseWeapon) stops too [design: the page's "current AI behaviour";
    // the shipped scripts call ResetAI to end one, FalloutNV.esm SCPT 0x000E07CE].
    g.drop_weapon_use(r);
    if g.runs_packages() {
        g.evaluate_package(r);
    } else if let Some(e) = g.extra.get_mut(&r) {
        // The runner's inputs choose packages: the choice is asked for again.
        e.current_package = None;
    }
    none()
}

/// The GetCurrentAIPackage code of a package type [documented: GECK wiki
/// GetCurrentAIPackage's table: 1 FOLLOW, 2 ESCORT, 3 EAT, 4 SLEEP, 13 WANDER, 14
/// TRAVEL, 15 ACCOMPANY, 16 USEITEMAT, 17 AMBUSH, 18 FLEE_NON_COMBAT, 33 USE_WEAPON, 34
/// DIALOGUE, 35 GUARD, 36 SANDBOX, 37 PATROL; hypothesis: Find is 0 EXPLORE, the table
/// having no FIND, and a Dialogue package is the second DIALOGUE (34), 6 being the
/// conversation itself]. The original agreed for Sandbox 36, Dialogue 34, Travel 14 and
/// Patrol 37 [observed: function probes `fp7_packages_locks` 2026-10-17, each actor's
/// current package found with GetIsCurrentPackage].
pub fn package_code(t: PackageType) -> i32 {
    match t {
        PackageType::Find => 0,
        PackageType::Follow => 1,
        PackageType::Escort => 2,
        PackageType::Eat => 3,
        PackageType::Sleep => 4,
        PackageType::Wander => 13,
        PackageType::Travel => 14,
        PackageType::Accompany => 15,
        PackageType::UseItemAt => 16,
        PackageType::Ambush => 17,
        PackageType::FleeNotCombat => 18,
        PackageType::UseWeapon => 33,
        PackageType::Dialogue => 34,
        PackageType::Guard => 35,
        PackageType::Sandbox => 36,
        PackageType::Patrol => 37,
        PackageType::Other(_) => -1,
    }
}

/// What the actor does over its package: in a conversation (with the player, or a
/// conversation between two actors), in combat, searching, or the package.
enum Doing {
    Talking,
    Fighting,
    Searching,
    Package(Option<PackageType>),
}

fn doing(g: &mut Game, r: FormId) -> Doing {
    if g.dialogue.as_ref().is_some_and(|d| d.actor == r) {
        return Doing::Talking;
    }
    // Both sides of a conversation between actors talk [hypothesis: the IDLE tree's
    // branch for such conversations (NPCtoNPC) lies under a root that wants the dialogue
    // procedure or the dialogue menu, so it is reached only if they do;
    // docs/spec-notes/idles.md §12].
    if g.speech.iter().any(|s| s.conversation.is_some() && (s.speaker == r || s.target == Some(r))) {
        return Doing::Talking;
    }
    match g.awareness.state(r) {
        State::Combat => return Doing::Fighting,
        State::Alert | State::Lost => return Doing::Searching,
        State::Normal => {}
    }
    let p = g.current_package(r);
    Doing::Package(p.and_then(|p| g.data.package_defs.get(&p)).map(|d| d.ty))
}

/// `actor.GetCurrentAIPackage`: the kind of package the actor runs, by the codes of
/// [`package_code`]; 5 COMBAT in combat, 29 SEARCH_FOR_ATTACKER while it searches
/// (Alert, Lost), 6 DIALOGUE in a conversation with the player [hypothesis: the
/// game's combat, search and conversation run as packages of their own, as the table's
/// COMBAT, SEARCH_FOR_ATTACKER and DIALOGUE suggest]; 0 with no package [observed:
/// Trudy and Joe Cobb, none of whose packages was current, read 0, `fp7_packages_locks`].
pub fn get_current_ai_package(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let code = match doing(g, r) {
        Doing::Talking => 6,
        Doing::Fighting => 5,
        Doing::Searching => 29,
        Doing::Package(t) => t.map_or(0, package_code),
    };
    Ok(Value::Num(code as f64))
}

/// `actor.GetCurrentAIProcedure`: what the actor is doing within its package, by the
/// codes of the GECK wiki's table [documented: GECK wiki GetCurrentAIProcedure: 0
/// TRAVEL, 2 ACQUIRE, 3 WAIT, 4 DIALOGUE, 7 WANDER, 8 SLEEP, 10 EAT, 11 FOLLOW, 12
/// ESCORT, 13 COMBAT, 17 DONE, 21 GET_UP, 27 ACCOMPANY, 28 USE_ITEM_AT, 30 WAIT_AMBUSH, 34
/// FLEE_NON_COMBAT, 36 SEARCH, 41 PATROL, 42 USE_WEAPON, 44 GUARD, 45 SANDBOX, 48
/// SITTING; "a Travel package travels, then waits at its destination"]. Chosen here:
///
/// - a conversation with the player or between two actors 4, combat 13, Alert or Lost 36;
/// - getting off a seat or bed 21, lying on a bed 8, sitting 48;
/// - a package whose goal was met 17 DONE [hypothesis: the end scripts of eight
///   packages of FalloutNV.esm (0x0012FEED-F0, 0x0016954E-51) wait for four actors' 17];
/// - else by the package type: Travel 0, Sleep 0 (on its way to the bed), Find 2,
///   Wander 7, Eat 10, Follow 11, Escort 12, Accompany 27, Use Item At 28, Ambush 30,
///   Flee Not Combat 34, Patrol 41, Use Weapon 42, Guard 44, Sandbox 45 [hypothesis:
///   each package's main procedure]; Dialogue 43 [observed: Sunny Smiles on her Dialogue
///   package, near and far, captures `fa_batch2_loaded_npcs` and `fp7_packages_locks`;
///   not in the wiki's table];
/// - 0 with no package [observed: Trudy and Joe Cobb, `fp7_packages_locks`].
pub fn get_current_ai_procedure(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let furniture = g.furniture_use(r);
    let goal_met = g.extra.get(&r).is_some_and(|e| e.package_goal_met);
    let code = match doing(g, r) {
        Doing::Talking => 4,
        Doing::Fighting => 13,
        Doing::Searching => 36,
        Doing::Package(t) => match furniture {
            Some(u) if u.state == 4 => 21,
            Some(u) if u.state == 3 && bed(&u) => 8,
            Some(u) if u.state == 3 => 48,
            _ if goal_met => 17,
            _ => t.map_or(0, procedure_code),
        },
    };
    Ok(Value::Num(code as f64))
}

/// The main procedure of a package type (see [`get_current_ai_procedure`]).
pub fn procedure_code(t: PackageType) -> i32 {
    match t {
        PackageType::Travel | PackageType::Sleep => 0,
        PackageType::Find => 2,
        PackageType::Dialogue => 43,
        PackageType::Wander => 7,
        PackageType::Eat => 10,
        PackageType::Follow => 11,
        PackageType::Escort => 12,
        PackageType::Accompany => 27,
        PackageType::UseItemAt => 28,
        PackageType::Ambush => 30,
        PackageType::FleeNotCombat => 34,
        PackageType::Patrol => 41,
        PackageType::UseWeapon => 42,
        PackageType::Guard => 44,
        PackageType::Sandbox => 45,
        PackageType::Other(_) => -1,
    }
}

/// `actor.Look target [rotateBody]`: the actor's head tracks the target, over all
/// other head tracking, until StopLook or the target leaves the loaded area; with
/// `rotateBody` 1 the whole body turns to it [documented: GECK wiki Look]. Kept here
/// ([`Game::script_look`]); the engine turns the head (`nvforever` `script_look.rs`).
/// The body turn is kept but not acted on yet.
pub fn look(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let target = a.form(0)?;
    let rotate_body = a.num_or(1, 0.0) != 0.0;
    g.senses.looks.insert(r, Look { target, rotate_body });
    none()
}

/// `actor.StopLook`: the forced head tracking of Look ends; normal head tracking
/// resumes [documented: GECK wiki StopLook].
pub fn stop_look(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    g.senses.looks.remove(&r);
    none()
}

/// `actor.SetPackDuration minutes`: the actor's current package runs for that many game
/// minutes [documented: GECK wiki SetPackDuration, "useful … for less than an hour"].
/// Counted from the call [hypothesis: from the call rather than from the package's start;
/// the two agree for the shipped calls in begin scripts, and the Hidden Valley restroom
/// trigger's `enteringActor.SetPackDuration 1` (`SCPT 0x000CE229`) only makes sense
/// from the call, [observed: decoded scripts]]. When the time is up the package ends
/// ([`Game::end_timed_packages`]). Without a current package nothing happens.
pub fn set_pack_duration(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let minutes = a.num(0)?.max(0.0);
    let now = g.world.calendar().map(|c| c.days_passed as f64).unwrap_or(0.0);
    if let Some(p) = g.extra.get(&r).and_then(|e| e.current_package) {
        let e = g.extra.entry(r).or_default();
        e.package_ends = Some((p, now + minutes / (24.0 * 60.0)));
        e.package_ends_by_schedule = false;
    }
    none()
}

/// `actor.SetActorsAI flag`: 0 stops the actor's AI, any other value starts it again;
/// its scripts still run, but it does not process packages, detection, Havok or
/// animation [documented: GECK wiki SetActorsAI]. Here: the periodic package choice
/// leaves it out (`crate::ai`), the AI that carries packages out does not step it
/// (`fnv_ai` `Ai::step_actor`: no walk, no sandbox), and the engine does not let it
/// detect (`nvforever` `detection.rs`). Its animation and collision are not stopped
/// [engineering choice: no engine switch for them yet; the actor stands in its idle].
pub fn set_actors_ai(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    g.extra.entry(r).or_default().ai_off = a.num(0)? == 0.0;
    none()
}
