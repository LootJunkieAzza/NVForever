//! What each actor's body does, told to the rules every frame (`fnv_game::anim_state`,
//! docs/design/script-stubs.md §3.4): GetAnimAction's action, the way IsMoving reads and
//! the movement group IsAnimPlaying reads, for the drawn actors and the player.
//!
//! - **NPCs and creatures** (after `locomotion::drive` has posed them): the attack or
//!   reload clip playing (`attack_anims::Attack::action`: a reload 9, a shot 2/3/4, a
//!   blow 2/4, a blocked blow's recoil 8, a block's reaction 7), else blocking 7, else
//!   drawing (0) or putting away (1) the weapon, else a crippled limb's stagger
//!   (`Flinch`), a PlayIdle idle, a package's begin, end or change idle, a cowering idle
//!   or a script's PlayGroup group: 13 [documented: GECK wiki GetAnimAction, 13 is
//!   "furniture anim, playidle anims, staggering from crippled limbs"]. Fidgets,
//!   collections' idles and conversation gestures are not 13 [observed: the AI observer's
//!   sitters and talkers read −1 nearly always]. The way it moves is its locomotion's
//!   velocity in its own axes, and the group its locomotion's clip (`Forward`,
//!   `FastLeft` …) while it walks, else `Idle` (a turn in place included).
//! - **The player**: the weapon's action from the combat state
//!   (`nvf_play::combat::Combat::player_anim_action`), else a PlayIdle idle 13; the way
//!   it moves is the movement keys ("requesting to move" [documented: GECK wiki
//!   IsMoving]), the group the walking or running group of that way, and whether it aims
//!   down a gun's sights (IsPlayerActionActive 9).
//!
//! At exit `anim state summary` counts how often each action began on an actor (the
//! smoke test `anim-state` reads it).
//!
//! Getting on and off furniture, getting up after a knockdown and thrown weapons' throw
//! phases are not told (they read as their other states) [gap left].

use crate::attack_anims::CombatPose;
use crate::idles::{ActorIdle, PlayerIdles};
use crate::limbs::ActorRig;
use crate::locomotion::{Dying, Flinch, Locomotion};
use crate::weapons::WeaponHold;
use bevy::prelude::*;
use fnv_actor::weapon::Stage;
use fnv_game::anim_state::{AnimAction, AnimState, MoveDir};
use fnv_game::idles::IdleSource;
use fnv_sim::FormId;
use nvf_play::combat::{Combat, WeaponAnim};
use nvf_play::control::PlayerControl;
use nvf_play::player::MoveIntent;
use nvf_play::script::ScriptState;
use std::collections::{BTreeMap, HashMap, HashSet};

/// Below this speed (units per second, the actor's own axes) a body is not walking
/// [engineering choice: detection.rs's threshold].
const MOVING_SPEED: f32 = 10.0;

pub struct AnimStatePlugin;

impl Plugin for AnimStatePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AnimStats>()
            .add_systems(Update, tell.after(crate::locomotion::drive).after(nvf_play::player::BodySet).after(crate::idles::drive))
            .add_systems(Last, report);
    }
}

/// What was told, for the summary at exit.
#[derive(Resource, Default)]
struct AnimStats {
    /// The action each actor was last told doing (−1 none).
    last: HashMap<u32, i32>,
    /// How often each action began, by code.
    begun: BTreeMap<i32, u32>,
    /// How often an actor began to move each way (IsMoving's codes 1 to 4).
    ways: BTreeMap<u8, u32>,
    last_way: HashMap<u32, u8>,
    actors: HashSet<u32>,
}

impl AnimStats {
    fn note(&mut self, actor: u32, s: &AnimState) {
        if self.actors.insert(actor) {
            info!("anim state: {actor:08X} first told: group {}, way {}, action {}", s.group.unwrap_or("none"), s.moving.code(), s.action.map_or(-1, AnimAction::code));
        }
        let code = s.action.map_or(-1, AnimAction::code);
        if self.last.insert(actor, code).unwrap_or(-1) != code && code >= 0 {
            *self.begun.entry(code).or_default() += 1;
        }
        let way = s.moving.code();
        if self.last_way.insert(actor, way).unwrap_or(0) != way && way > 0 {
            *self.ways.entry(way).or_default() += 1;
        }
    }
}

fn report(mut exit: MessageReader<AppExit>, st: Res<AnimStats>) {
    if exit.read().next().is_none() {
        return;
    }
    let name = |c: i32| match c {
        0 => "equip",
        1 => "unequip",
        2 => "attack",
        3 => "follow-through",
        4 => "attack return",
        7 => "block",
        8 => "stagger",
        9 => "reload",
        _ => "script anim",
    };
    let begun: Vec<String> = st.begun.iter().map(|(c, n)| format!("{} ({c}) {n}", name(*c))).collect();
    let ways: Vec<String> = st.ways.iter().map(|(c, n)| format!("{c}: {n}")).collect();
    info!("anim state summary: {} actors told; actions begun: {}; ways begun: {}", st.actors.len(), if begun.is_empty() { "none".into() } else { begun.join(", ") }, if ways.is_empty() { "none".into() } else { ways.join(", ") });
}

type Body = (&'static ActorRig, &'static InheritedVisibility, Option<&'static Locomotion>, Option<&'static WeaponHold>, Option<&'static CombatPose>, Option<&'static ActorIdle>, Option<&'static Flinch>, Has<Dying>);

/// The idles that are GetAnimAction 13 (module notes).
fn special(source: IdleSource) -> bool {
    matches!(source, IdleSource::Script | IdleSource::PackageBegin | IdleSource::PackageEnd | IdleSource::PackageChange | IdleSource::Cower)
}

/// The movement group of a way and pace (`fnv_actor::locomotion::Move::group`).
fn group(dir: MoveDir, fast: bool) -> &'static str {
    match (dir, fast) {
        (MoveDir::None, _) => "Idle",
        (MoveDir::Forward, false) => "Forward",
        (MoveDir::Backward, false) => "Backward",
        (MoveDir::Left, false) => "Left",
        (MoveDir::Right, false) => "Right",
        (MoveDir::Forward, true) => "FastForward",
        (MoveDir::Backward, true) => "FastBackward",
        (MoveDir::Left, true) => "FastLeft",
        (MoveDir::Right, true) => "FastRight",
    }
}

#[allow(clippy::too_many_arguments)]
fn tell(
    time: Res<Time>,
    mut script: Option<ResMut<ScriptState>>,
    actors: Query<Body>,
    combat: Option<Res<Combat>>,
    weapon: Option<Res<WeaponAnim>>,
    control: Option<Res<PlayerControl>>,
    intent: Option<Res<MoveIntent>>,
    player_idles: Option<Res<PlayerIdles>>,
    mut stats: ResMut<AnimStats>,
    mut told: Local<HashSet<u32>>,
) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let g = &mut rt.game;
    let dt = time.delta_secs().min(0.1);
    let clock = time.elapsed_secs();
    let mut now = HashSet::new();
    for (rig, vis, lo, hold, pose, idle, flinch, dying) in &actors {
        if rig.reference == FormId::PLAYER_REF.0 || !vis.get() || dying {
            continue;
        }
        let action = pose
            .and_then(|p| p.attack.as_ref().and_then(|a| a.action(dt)).or(p.blocking.then_some(AnimAction::Block)))
            .or(match hold.map(|h| h.anim.stage) {
                Some(Stage::Equipping) => Some(AnimAction::Equip),
                Some(Stage::Unequipping) => Some(AnimAction::Unequip),
                _ => None,
            })
            .or_else(|| flinch.filter(|f| clock < f.at + f.clip.clip.length()).map(|_| AnimAction::ForceScriptAnim))
            .or_else(|| idle.and_then(|i| i.0.playing()).filter(|p| p.group.is_some() || special(p.source)).map(|_| AnimAction::ForceScriptAnim));
        // Not walking, it plays Idle, a turn in place included: the locomotion's clip
        // is TurnLeft or TurnRight while a standing actor turns (to face the player, a
        // spot turn), and Chet standing at his counter read `IsAnimPlaying Idle` 0 so
        // [observed in NVForever: smoke anim-state, 2026-10-18; hypothesis: the
        // original's standing actor reads Idle 1 while it turns, as Sunny Smiles standing
        // read 1].
        let (moving, group) = match lo {
            Some(l) => {
                let way = MoveDir::of(l.velocity[0], l.velocity[1], MOVING_SPEED);
                (way, Some(if way == MoveDir::None { "Idle" } else { l.blend.cur.mv.group() }))
            }
            None => (MoveDir::None, None),
        };
        let st = AnimState { action, moving, group, iron_sights: false, at: 0.0 };
        stats.note(rig.reference, &st);
        g.set_anim_state(FormId(rig.reference), st);
        now.insert(rig.reference);
    }
    // The player.
    let (trigger, aiming, walk) = (control.as_deref().is_some_and(|c| c.trigger), control.as_deref().is_some_and(|c| c.aiming), intent.as_deref().is_some_and(|i| i.walk));
    let weapon_action = combat.as_deref().zip(weapon.as_deref()).and_then(|(c, w)| c.player_anim_action(w, trigger, aiming, dt));
    let idle_action = player_idles
        .as_deref()
        .and_then(|pi| pi.first.playing().or(pi.third.playing()))
        .filter(|p| p.group.is_some() || special(p.source))
        .map(|_| AnimAction::ForceScriptAnim);
    let moving = intent.as_deref().map_or(MoveDir::None, |i| MoveDir::of(i.right, i.forward, 0.01));
    let iron_sights = combat.as_deref().zip(weapon.as_deref()).is_some_and(|(c, w)| c.player_sights_up(w, aiming));
    let st = AnimState { action: weapon_action.or(idle_action), moving, group: Some(group(moving, !walk)), iron_sights, at: 0.0 };
    stats.note(FormId::PLAYER_REF.0, &st);
    g.set_anim_state(FormId::PLAYER_REF, st);
    // Actors no longer drawn are forgotten (they read as doing nothing).
    for gone in told.difference(&now) {
        g.forget_anim_state(FormId(*gone));
    }
    *told = now;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_follow_the_way_and_pace() {
        assert_eq!(group(MoveDir::None, true), "Idle");
        assert_eq!(group(MoveDir::Backward, false), "Backward");
        assert_eq!(group(MoveDir::Left, true), "FastLeft");
    }

    #[test]
    fn only_special_idles_are_13() {
        assert!(special(IdleSource::Script));
        assert!(special(IdleSource::Cower));
        assert!(!special(IdleSource::Fidget));
        assert!(!special(IdleSource::Gesture));
        assert!(!special(IdleSource::MarkerIdles));
    }
}
