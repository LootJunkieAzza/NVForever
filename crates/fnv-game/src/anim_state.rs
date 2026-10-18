//! What each actor's body is doing, as the engine tells the rules every frame
//! (docs/design/script-stubs.md §3.4): the special animation action (GetAnimAction), the
//! way it asks to move (IsMoving), the movement group its body plays (IsAnimPlaying) and,
//! for the player, the actions IsPlayerActionActive asks about.
//!
//! The engine animates; this module keeps the latest snapshot per actor
//! ([`AnimState`], `Senses::anim`) and the rules that turn an attack's or a reload's time
//! into GetAnimAction's codes ([`shot_action`], [`swing_action`]), so the engine's player
//! side (`nvf-play` combat) and NPC side (`nvforever` anim_state.rs) number them alike. A
//! snapshot holds until the engine tells the next one or forgets the actor (it left the
//! view: unloaded, hidden, dying), never by age: a clock-based expiry read Chet's
//! snapshot as gone while the console was open (smoke `anim-state`, 2026-10-18). With
//! no engine (a headless run) nothing is told and every actor reads as doing nothing
//! [engineering choice].
//!
//! The codes are New Vegas's [documented: GECK wiki GetAnimAction, the table "revised for
//! GECK NV"], checked against the AI observer's campaigns [observed: `anim_action` of the
//! Goodsprings gunfight and the melee-spacing runs, C:\fnv-private\capture,
//! docs/spec-notes/anim-state.md]. Codes no source explains are not answered (module
//! notes of [`AnimAction`]).

use crate::functions::{none, yes, Args, Ctx};
use crate::game::Game;
use fnv_script::vm::Value;
use fnv_sim::FormId;

/// GetAnimAction's codes [documented: GECK wiki GetAnimAction, New Vegas table]. Not
/// answered: 10 (dodge), 11 and 12 (the page: "may be left overs"); 6 (the throwable
/// left the hand) and 5 (gearing up to throw) are not told apart from a shot, as
/// NVForever's thrown weapons have no throw phases yet; 17, which the observer saw on
/// .357 revolvers for about two seconds before a reload's 9, has no documented meaning
/// [observed: Goodsprings gunfight, `WeapNV357Revolver` 0008F216] and is not answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimAction {
    /// 0: drawing the weapon.
    Equip,
    /// 1: putting it away.
    Unequip,
    /// 2: "firing full auto or doing a melee swing"; also the start of a single shot
    /// [observed: 2 for about four frames before each semi-automatic shot's 3].
    Attack,
    /// 3: "returned when firing semi auto" [documented]: the frame of the shot
    /// [observed: one frame for the .357 revolver and the single shotgun; the bolt-action
    /// varmint and hunting rifles held it about 0.45 s, through the bolt, which is not
    /// reproduced: hypothesis left].
    FollowThrough,
    /// 4: "returning to the static animation after the attack" [documented]: the rest
    /// of a shot's or a swing's clip [observed: about 0.33 s after a shot, after each
    /// creature's and the baseball bat's 2].
    AttackReturn,
    /// 7: blocking.
    Block,
    /// 8: "staggered by an npc that is blocking" [documented]: the attacker's recoil.
    Stagger,
    /// 9: reloading [documented; observed: about two seconds after a magazine's last
    /// shot, every gun of the gunfight].
    Reload,
    /// 13: "every action not included in the ones above": furniture, PlayIdle idles,
    /// staggering from crippled limbs, getting up [documented].
    ForceScriptAnim,
}

impl AnimAction {
    pub fn code(self) -> i32 {
        match self {
            AnimAction::Equip => 0,
            AnimAction::Unequip => 1,
            AnimAction::Attack => 2,
            AnimAction::FollowThrough => 3,
            AnimAction::AttackReturn => 4,
            AnimAction::Block => 7,
            AnimAction::Stagger => 8,
            AnimAction::Reload => 9,
            AnimAction::ForceScriptAnim => 13,
        }
    }
}

/// IsMoving's codes [documented: GECK wiki IsMoving].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MoveDir {
    #[default]
    None,
    Forward,
    Backward,
    Left,
    Right,
}

impl MoveDir {
    pub fn code(self) -> u8 {
        match self {
            MoveDir::None => 0,
            MoveDir::Forward => 1,
            MoveDir::Backward => 2,
            MoveDir::Left => 3,
            MoveDir::Right => 4,
        }
    }

    /// The way of a movement in the actor's own axes (`right`, `forward`, any units):
    /// the larger of the two, forward and back on a tie [hypothesis: the page gives one
    /// code for a diagonal]; none when both are below `min`.
    pub fn of(right: f32, forward: f32, min: f32) -> MoveDir {
        if right.abs() < min && forward.abs() < min {
            MoveDir::None
        } else if forward.abs() >= right.abs() {
            if forward > 0.0 {
                MoveDir::Forward
            } else {
                MoveDir::Backward
            }
        } else if right > 0.0 {
            MoveDir::Right
        } else {
            MoveDir::Left
        }
    }
}

/// One actor's body as the engine last told it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AnimState {
    /// GetAnimAction: the special animation action; `None` reads −1.
    pub action: Option<AnimAction>,
    /// IsMoving: the way the actor asks to move (the player: the movement keys; an NPC:
    /// the way its body walks).
    pub moving: MoveDir,
    /// The movement group its body plays (`Idle`, `Forward`, `FastLeft`: the clip's
    /// sequence name, `fnv_actor::locomotion::Move::group`), for IsAnimPlaying.
    pub group: Option<&'static str>,
    /// The player only: aiming down a gun's sights (IsPlayerActionActive 9).
    pub iron_sights: bool,
    /// When ([`Game::time`]) it was told (for traces; it does not expire).
    pub at: f64,
}

/// GetAnimAction of a shot `t` seconds into its clip of `len` seconds, the shot leaving
/// at `fire` (0: at the start), with this frame's step `dt`: 2 until the shot (at least
/// the first frame), 3 the frame after, 4 for the rest of the clip, then none
/// [observed: 2 for a few frames, 3 one frame, 4 about 0.33 s on semi-automatic guns;
/// hypothesis: the boundaries are the clip's shot and end]. `repeating`: an automatic
/// weapon's shots keep coming (the trigger held, a looping clip in its loop): 2
/// throughout [documented: 2 is "firing full auto"].
pub fn shot_action(t: f32, fire: f32, len: f32, dt: f32, repeating: bool) -> Option<AnimAction> {
    let shot = fire.max(dt);
    if repeating || t < shot {
        Some(AnimAction::Attack)
    } else if t < shot + dt {
        Some(AnimAction::FollowThrough)
    } else if t < len {
        Some(AnimAction::AttackReturn)
    } else {
        None
    }
}

/// GetAnimAction of a melee swing (or a creature's blow) `t` seconds into its clip of
/// `len` seconds that strikes at `hit`: 2 until the strike, 4 after it [observed: the
/// creatures' and the baseball bat's blows read 2, then 4, without 3; hypothesis: the
/// boundary is the clip's `Hit` key].
pub fn swing_action(t: f32, hit: f32, len: f32) -> Option<AnimAction> {
    if t < hit {
        Some(AnimAction::Attack)
    } else if t < len {
        Some(AnimAction::AttackReturn)
    } else {
        None
    }
}

impl Game<'_> {
    /// The engine tells what an actor's body does now (every frame, for the actors it
    /// draws and the player).
    pub fn set_anim_state(&mut self, actor: FormId, mut s: AnimState) {
        s.at = self.time;
        self.senses.anim.insert(actor, s);
    }

    /// The actor left the engine's view: forget its body's state.
    pub fn forget_anim_state(&mut self, actor: FormId) {
        self.senses.anim.remove(&actor);
    }

    /// The actor's body as last told, until the engine forgets it.
    pub fn anim_state(&self, actor: FormId) -> Option<&AnimState> {
        self.senses.anim.get(&actor)
    }
}

/// `actor.GetAnimAction`: the special animation action the actor is doing, −1 for none
/// [documented: GECK wiki GetAnimAction; observed: −1 for the player and Sunny Smiles at
/// rest (capture `fa_batch1_queries`), 0 for Ringo drawing his weapon 3 frames after
/// SetAlert 1 and −1 once it was out (`fa_batch3_actions`)]. Read from the engine's
/// snapshot ([`Game::anim_state`]); an actor it does not draw reads −1.
pub fn get_anim_action(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let code = g.anim_state(r).and_then(|s| s.action).map_or(-1, AnimAction::code);
    Ok(Value::Num(code as f64))
}

/// `IsPlayerActionActive index`: 1 while the player performs the action [documented:
/// GECK wiki IsPlayerActionActive]. Answered: 9, `PLAYER_ACTION_IRON_SIGHTS`, aiming down
/// a gun's sights, and not while the player is in combat [documented: the page's notes,
/// "will not return true if the player is in combat"] (both shipped calls, in Old World
/// Blues, ask 9: docs/design/script-function-audit.md). The page's ActionIndex list is
/// a template the wiki export lacks, so the other indices' meanings are not known here
/// (8 is "standing on a table or chair" by the page's example, which NVForever's player
/// never does): they read 0 [not answered].
pub fn is_player_action_active(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let index = a.num_or(0, 0.0) as i32;
    if index != 9 {
        return none();
    }
    let sights = g.anim_state(FormId::PLAYER_REF).is_some_and(|s| s.iron_sights);
    yes(sights && !g.awareness.hunted(FormId::PLAYER_REF))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_semi_automatic_shot_reads_2_then_3_then_4() {
        let dt = 1.0 / 60.0;
        let seq: Vec<Option<i32>> = (0..30).map(|i| shot_action(i as f32 * dt, 0.05, 0.4, dt, false).map(AnimAction::code)).collect();
        assert_eq!(seq[0], Some(2));
        assert_eq!(seq[2], Some(2));
        let first3 = seq.iter().position(|c| *c == Some(3)).unwrap();
        assert_eq!(seq.iter().filter(|c| **c == Some(3)).count(), 1, "3 lasts one frame");
        assert!(seq[first3 + 1..].iter().take_while(|c| c.is_some()).all(|c| *c == Some(4)));
        assert_eq!(shot_action(0.5, 0.05, 0.4, dt, false), None);
    }

    #[test]
    fn a_shot_leaving_at_the_start_still_reads_2_on_its_first_frame() {
        let dt = 1.0 / 60.0;
        assert_eq!(shot_action(0.0, 0.0, 0.4, dt, false), Some(AnimAction::Attack));
        assert_eq!(shot_action(dt, 0.0, 0.4, dt, false), Some(AnimAction::FollowThrough));
        assert_eq!(shot_action(3.0 * dt, 0.0, 0.4, dt, false), Some(AnimAction::AttackReturn));
    }

    #[test]
    fn full_auto_reads_2_throughout() {
        assert_eq!(shot_action(0.3, 0.0, 0.4, 0.016, true), Some(AnimAction::Attack));
    }

    #[test]
    fn a_swing_reads_2_until_its_hit_then_4() {
        assert_eq!(swing_action(0.1, 0.4, 0.9), Some(AnimAction::Attack));
        assert_eq!(swing_action(0.5, 0.4, 0.9), Some(AnimAction::AttackReturn));
        assert_eq!(swing_action(1.0, 0.4, 0.9), None);
    }

    #[test]
    fn directions_take_the_larger_axis() {
        assert_eq!(MoveDir::of(0.0, 0.0, 1.0), MoveDir::None);
        assert_eq!(MoveDir::of(0.5, -0.2, 1.0), MoveDir::None);
        assert_eq!(MoveDir::of(10.0, 50.0, 1.0).code(), 1);
        assert_eq!(MoveDir::of(10.0, -50.0, 1.0).code(), 2);
        assert_eq!(MoveDir::of(-80.0, 20.0, 1.0).code(), 3);
        assert_eq!(MoveDir::of(80.0, 20.0, 1.0).code(), 4);
        assert_eq!(MoveDir::of(1.0, 1.0, 0.5), MoveDir::Forward);
    }

    #[test]
    fn codes_are_the_new_vegas_table() {
        let all = [
            (AnimAction::Equip, 0),
            (AnimAction::Unequip, 1),
            (AnimAction::Attack, 2),
            (AnimAction::FollowThrough, 3),
            (AnimAction::AttackReturn, 4),
            (AnimAction::Block, 7),
            (AnimAction::Stagger, 8),
            (AnimAction::Reload, 9),
            (AnimAction::ForceScriptAnim, 13),
        ];
        for (a, c) in all {
            assert_eq!(a.code(), c);
        }
    }
}
