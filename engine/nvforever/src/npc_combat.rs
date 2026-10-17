//! Combat AI, step CA1 (docs/design/combat-ai.md): actors in Combat
//! (`fnv_game::awareness`) attack their target with the same rules as the player
//! (`nvf_play::combat`).
//!
//! Each drawn, living actor in Combat with a target:
//! - **What it fights with**: the weapon drawn on it (`HeldWeapon`); a gun (a weapon
//!   that fires a projectile) is a ranged attack, any other weapon a melee one; without
//!   a weapon a creature attacks with its own damage and reach (CREA DATA Damage, RNAM
//!   Attack Reach), an NPC with its fists [design].
//! - **How near it has to be** ([`engage`]), told to the AI that moves actors
//!   (`Awarenesses::engage`, `fnv_ai::alert` closes in): a gun's useful range, a melee
//!   weapon's or creature's reach [design].
//! - **When it fires**: only while it detects and sees its target (the latest
//!   detection of the pair: above 0, in sight), after a reaction time from the moment it
//!   first saw it in this fight, with its weapon drawn, and a gun only facing it (within
//!   [`FIRE_FACING`] of straight ahead; owner's playtest list 5 item 33). A gun keeps its own cycle
//!   (`fnv_combat::attack::Cycle`: fire rate, magazine, reload) as the player's does;
//!   the trigger is held for automatic weapons and pulled once a shot for the others
//!   [design].
//! - **Where it aims**: from its eyes at the target's chest, the shot drawn from its
//!   weapon's muzzle (the model's `ProjectileNode`, `weapons::Muzzle`) closing onto that
//!   line, as the player's first-person shots (`ActorFire`; owner 2026-10-11: "the shots
//!   were coming from Violet's face, not the gun"); without a muzzle node, from the eyes.
//!   Inside its spread: the documented actor
//!   spread for its weapon skill, standing, plus an aim that settles over
//!   [`SETTLE_SECONDS`] of firing at the same target [design].
//! - **Aiming down the sights** (owner's playtest list 2026-10-11: "only if the target
//!   is at a distance; anything less than 20 metres they fire from the hip"): a gun
//!   with sights (WEAP Sight FOV above 0; 0 "essentially disables iron sights"
//!   [documented: GECK wiki Weapons]) is aimed down its sights while the actor fires at
//!   a target more than [`HIP_FIRE_WITHIN`] away (20 m at 70 units per metre,
//!   docs/spec-notes/physics.md §6.3), from the hip nearer ([`aims_down_sights`]).
//!   Aimed, the spread loses the unaimed penalty (fUnaimedSpreadPenalty) [documented:
//!   GECK wiki Gun Spread Formula (New Vegas)], the actor plays its weapon's iron-sights
//!   aim and attack clips (`2hraimis.kf`, `2hrattack3is.kf`, `attack_anims.rs`), and it
//!   walks instead of running (`Awarenesses::aiming`, `fnv_ai::alert`). The record's
//!   Sight Usage ("the percentage of fCombatIronSightsDistance" beyond which an actor
//!   uses its sights [documented: GECK wiki Weapons]) is not used: the owner's 20 m
//!   rule stands in for it, and fCombatIronSightsDistance is neither in FalloutNV.esm
//!   nor on the wiki.
//! - **How it looks** (`attack_anims.rs`): each shot and blow asks for its attack clip
//!   ([`PlayAttack`]); a blow's damage is dealt at its clip's `Hit` key
//!   ([`AttackTimes`]), and lands if the target is still within reach then [design].
//!   Each fighter's pitch to its target ([`Fighters::pitch`]) bends its aim.
//! - **Melee**: it closes in until its body is [`MELEE_STOP_GAP`] from its target's
//!   (the bodies as `nvf_play::blocking` has them: a long creature reaches out in front
//!   of its feet), and strikes while the gap is within [`MELEE_HIT_GAP`] and it sees the
//!   target, a blow every [`STRIKE_SECONDS`] (divided by the weapon's attack speed)
//!   [design; owner 2026-10-11: a dog gets into range and bites, it does not push the
//!   player around]. Without bodies to measure, its reach from its feet counts.
//! - **Creatures' power attacks** (a radscorpion's sting): a creature's blow is a power
//!   attack by its combat style's Power Attack % Chance [documented: GECK wiki Combat
//!   Style], or [`FALLBACK_POWER_CHANCE`] where the style gives none [design: owner
//!   2026-10-16, random power attacks where the rule is not found]; which one by the
//!   style's normal and forward shares, evenly where both are 0 [design], among those it
//!   has clips for ([`AttackTimes::powers`]). It lands at its own clip's `Hit` key, deals
//!   the creature's usual damage [design: no power damage rule is sourced] and carries
//!   its attack group to the hit, for an unarmed attack effect that names it
//!   (`nvf_play::combat::hit_effects::applies`). Rolled on their own sequence.
//! - **Melee combos** (docs/design/combat-ai.md §15): a blow's clip with an `a:` text
//!   key (`a:L` on the `AttackRight` clips, `a:R` on the `AttackLeft` ones; `fnv_combat::
//!   melee`) brings the next melee decision forward to that key; a blow decided between
//!   the key and the clip's end goes on the combo with the group the key names (right,
//!   left, right…, fists alternating hands), as the player's swings do, cutting the clip
//!   short. A blow decided later, after a hold, a block or a recoil starts again from the
//!   weapon's own attack group. A clip without an `a:` key keeps the [`STRIKE_SECONDS`]
//!   pace [design; owner request 2026-10-16].
//!
//! - **Its combat style** (CSTY, `fnv_game::combat_style`; docs/design/combat-ai.md
//!   §10.4) [documented: GECK wiki Combat Style]: Weapon Restrictions "Melee Only" has
//!   it fight a gun-holder with its fists; the Ranged Weapon Max Range Mult scales the
//!   weapon's max range in the distance it fights from; a semi-automatic gun waits a
//!   random time between the weapon's Semi-Auto Fire Delay Min and Max, times the
//!   style's multipliers, between shots ("semi-auto weapon's fire rate when used by
//!   actors" [documented: GECK wiki Weapons]); in melee each time it may strike it makes
//!   the melee decision (attack chance with the hand-to-hand bonus and the attack skill
//!   modifier, under attack or not), and holds for the Hold Timer when it does not attack.
//! - **Confidence and fleeing** (`fnv_game::combat_group`, docs/design/combat-ai.md
//!   §12.2): each fighter's strength (health, damage per second) goes to the game every
//!   frame; once hurt, every random 0 to fCombatThreatRatioUpdateTime seconds it makes a
//!   flee check (its group's threat ratio, and its personal one when its style says so,
//!   against its confidence and this fight's modifier) and flees when it fails
//!   (`fnv_ai::alert` runs it to a door, cover or away). It does not attack while it
//!   flees. At its flee spot, hurt or with a target it sees within
//!   fCombatFleeBoostConfidenceTargetRadius, its confidence goes up a level and it checks
//!   again: it fights again or flees on; after fCombatFleeWaitTime there it checks again
//!   without a boost [hypothesis: what the wait time is for].
//! - **Tactics** (`combat_tactics`): cover by the style's Take Cover Chance and timers,
//!   flank places for a group's gunmen, no shot while a friend stands in the line of
//!   fire, and a step back from a target nearer than [`tactics::BACK_OFF_WITHIN`]
//!   [design].
//! - **Moving while it fights**: it stays facing its target and strafes or backs away
//!   (nvforever `residents`, `Core::aims`), so it fires on the move.
//!
//! - **Attack lines**: each shot or blow it begins tries its Attack line, said at most
//!   once per its attack timer (`fnv_game::combat_lines`, docs/spec-notes/combat-dialogue.md).
//! - **Crouching, blocking, closing in** (people only; docs/design/combat-ai.md §14, the
//!   rules in `combat_stance`): a gunman that sees a far target now and then crouches to
//!   fire (`Awarenesses::crouching`), hides crouched behind low cover, shifts sideways
//!   after a while and stands up for a close-combat enemy coming in or to move; a melee
//!   fighter against a close-combat opponent makes the melee decision block first, also
//!   in reaction to the opponent's blow, and the attacker of a blocked blow recoils;
//!   against a gunman that sees it from afar it bounds from cover to cover, or charges
//!   when there is none, or stays by a gunman of its group until the target comes near.
//!
//! Whom it fights is the detection states' (its combat target). Not built: grenades,
//! ammunition use, weapon choice among several, dodging and power attacks of people.

use crate::attack_anims::{AttackTimes, Move, PlayAttack, Power, SwingPick, DEFAULT_HIT};
use crate::combat_stance::{self as stance, Temperament};
use crate::combat_tactics::{self as tactics, Battle, Phase, Tactic};
use crate::nav::NavState;
use fnv_game::combat_group::{Post, PostKind, Strength};
use fnv_nav::Space;
use nvf_play::Location;
use crate::limbs::ActorRig;
use crate::locomotion::{Dying, Flinch};
use crate::ragdoll::Corpse;
use crate::weapons::{HeldWeapon, Muzzle, WeaponHold};
use crate::world::bevy_to_game;
use bevy::prelude::*;
use fnv_actor::weapon::Stage;
use fnv_combat::attack::{ClipTiming, Cycle, Event, Input, Timing};
use fnv_combat::damage::AttackKind;
use fnv_combat::data::WeaponStats;
use fnv_combat::spread::{self, Stance};
use fnv_combat::vec::{self, V3};
use fnv_game::awareness::State;
use fnv_game::combat_style::{melee_attack_chance, melee_block_chance, Opponent};
use fnv_game::detection::Band;
use fnv_sim::FormId;
use nvf_play::blocking::ActorBlocks;
use nvf_play::combat::{ActorFire, ActorStrike, BlockedBlow, Combat, CombatStats};
use nvf_play::control::PlayerControl;
use nvf_play::phys::PhysState;
use nvf_play::player::{Player, EYE_HEIGHT};
use nvf_play::script::{with_world, ScriptState};
use nvf_play::sim::SimState;
use std::collections::{BTreeMap, HashMap};

/// Seconds between first seeing the target in a fight and the first attack [design].
pub const REACTION_SECONDS: f32 = 0.6;
/// Extra spread when an actor starts firing at a target, degrees, gone after
/// [`SETTLE_SECONDS`] of firing at it [design: the first shots of a burst miss more].
pub const SETTLE_DEGREES: f32 = 4.0;
pub const SETTLE_SECONDS: f32 = 3.0;
/// Seconds between two blows at attack speed 1 [design].
pub const STRIKE_SECONDS: f32 = 1.2;
/// A gun's engage distance: its max range (WEAP DNAM) times this, within these bounds;
/// without a max range the upper bound [design].
pub const GUN_RANGE_SHARE: f32 = 0.8;
pub const GUN_ENGAGE_MIN: f32 = 400.0;
pub const GUN_ENGAGE_MAX: f32 = 2500.0;
/// Melee: within this share of the reach the actor stops closing in [design].
pub const MELEE_ENGAGE_SHARE: f32 = 0.8;
/// Melee: the gap between the two bodies it closes to, and the gap within which its
/// blows land and beyond which it closes in again, game units [design].
pub const MELEE_STOP_GAP: f32 = 6.0;
pub const MELEE_HIT_GAP: f32 = 24.0;
/// A creature without a reach of its own reaches this far, game units [design].
pub const CREATURE_REACH: f32 = 96.0;
/// Within this distance of its target an actor fires from the hip, beyond it down its
/// sights: 20 m at 70 units per metre [owner's playtest list 2026-10-11; the units per
/// metre: observed, gravity fits 9.81 m/s² at 70 units/m, physics.md §6.3].
pub const HIP_FIRE_WITHIN: f32 = 1400.0;
/// Once aiming down its sights, an actor keeps them up until its target is this much
/// nearer than [`HIP_FIRE_WITHIN`], so it does not switch at the edge [design].
pub const SIGHTS_HYSTERESIS: f32 = 50.0;
/// Without a muzzle node, shots leave this far in front of the eyes, game units [design].
pub(crate) const MUZZLE_AHEAD: f32 = 24.0;
/// The chest as a fraction of the eye height (as `detection.rs` aims its sight).
pub(crate) const CHEST: f32 = 0.75;
/// A gunman fires only while its target is within this angle of straight ahead,
/// radians (30°) [design: owner's playtest list 5 item 33, "NPCs must face their target
/// before shooting"].
pub const FIRE_FACING: f32 = 30.0 * std::f32::consts::PI / 180.0;

/// How far (radians, 0 to π) the target at `to` lies off the heading of the actor whose
/// root is at `global`, standing at `feet` (game space).
pub(crate) fn facing_off(global: &GlobalTransform, feet: V3, to: V3) -> f32 {
    let ahead = bevy_to_game(global.transform_point(Vec3::Y)) - bevy_to_game(global.translation());
    let want = Vec2::new(to[0] - feet[0], to[1] - feet[1]);
    let have = ahead.truncate();
    if want.length() < 1.0 || have.length() < 1e-6 {
        return 0.0;
    }
    have.angle_to(want).abs()
}

/// The way an actor whose root is `global` faces, horizontal, game space.
fn heading(global: &GlobalTransform) -> [f32; 2] {
    let ahead = bevy_to_game(global.transform_point(Vec3::Y)) - bevy_to_game(global.translation());
    [ahead.x, ahead.y]
}

fn flat(a: V3, b: V3) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

pub struct NpcCombatPlugin;

impl Plugin for NpcCombatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Fighters>()
            .add_systems(Update, fight.after(nvf_play::PlaySet::Scripts).run_if(resource_exists::<Combat>))
            .add_systems(Last, report);
    }
}

/// How an actor attacks.
#[derive(Debug, Clone, PartialEq)]
enum Arm {
    Gun(FormId),
    Melee(Option<FormId>),
    /// A creature's own attack: damage and reach.
    Creature(f32, f32),
}

/// One actor in a fight.
struct Fighter {
    target: FormId,
    arm: Arm,
    /// Seconds since it first saw its target in this fight; seconds it has fired at it.
    seen_for: f32,
    firing_for: f32,
    cycle: Option<Cycle>,
    /// Seconds to its next blow.
    strike_in: f32,
    /// The trigger was pulled last frame (a semi-automatic weapon is let go between
    /// shots).
    pulled: bool,
    /// Blows on their way: seconds until each lands, and the blow.
    blows: Vec<(f32, ActorStrike)>,
    /// Aiming down its sights now; how it last fired while ready (down the sights or
    /// from the hip), for the log: it may lower its sights while not ready (going to
    /// cover) and come back ready near enough to fire from the hip.
    sights: bool,
    ready_sights: bool,
    /// Seconds before a semi-automatic gun may be fired again (its combat style's
    /// firing delay).
    pull_in: f32,
    /// This fight's confidence modifier; seconds to the next flee check.
    confidence_mod: f64,
    flee_check_in: f32,
    /// Cover, flank, line of fire (`combat_tactics`).
    tactic: Tactic,
    /// The start (`Flinch::at`, as bits) of the last hit reaction counted as holding it
    /// back.
    stagger_seen: Option<u32>,
    /// Its temperaments (docs/design/combat-ai.md §14.1), kept across targets.
    temper: Temperament,
    /// Crouched to fire since (clock); seconds stood still; its feet last frame; seconds
    /// to its next stance decision; seconds since it last saw its target (§14.2).
    crouched_since: Option<f32>,
    still_for: f32,
    last_feet: Option<V3>,
    stance_in: f32,
    unseen_for: f32,
    /// Blocking until (clock), raised against a blow (§14.3).
    block: Option<(f32, bool)>,
    /// A reaction to its opponent's blow: due at (clock), and when the blow lands.
    react: Option<(f32, f32)>,
    /// Its opponent's blows seen begun; its own blows begun.
    seen_opp_blows: u32,
    blows_begun: u32,
    /// It cannot strike until then (clock): its blow was blocked.
    recoil_until: f32,
    /// Closing in on a gunman (§14.4).
    approach: Approach,
    /// Its last melee blow's swing, for a combo (module notes).
    swing: Option<NpcSwing>,
}

/// A fighter's last melee blow (module notes).
#[derive(Debug, Clone, Copy, Default)]
struct NpcSwing {
    /// Seconds since it began.
    t: f32,
    /// Its clip's timing, once `attack_anims` started it.
    clip: Option<crate::attack_anims::SwingClip>,
    /// Blows in this combo so far, this one included.
    run: u32,
}

impl NpcSwing {
    /// What a blow begun now goes on with: the group its `a:` key named, between that key
    /// and the clip's end.
    fn next(&self) -> Option<fnv_combat::melee::NextAttack> {
        let c = self.clip?;
        c.chain.filter(|(at, _)| self.t >= *at && self.t <= c.len).map(|(_, n)| n)
    }
}

/// A melee fighter closing in (docs/design/combat-ai.md §14.4).
#[derive(Debug, Clone, Default)]
struct Approach {
    guard: Option<Guarding>,
    /// A guard ended in this fight (it does not guard again).
    guarded: bool,
    bound: Option<Bound>,
    /// Seconds to its next look for a bound.
    bound_in: f32,
    /// Charging in the open (logged once per run).
    charging: bool,
    /// The post it was given last frame.
    posted: Option<PostKind>,
}

#[derive(Debug, Clone, Copy)]
struct Guarding {
    friend: u32,
    since: f32,
    patience: f32,
    turn: f32,
    at: V3,
}

#[derive(Debug, Clone, Copy)]
struct Bound {
    at: V3,
    arrived: Option<f32>,
    pause: f32,
}

/// A fighter's pose this frame, for `attack_anims`.
#[derive(Debug, Clone, Copy, Default)]
struct Pose {
    /// Its pitch to its target (radians, negative looks down).
    pitch: f32,
    weapon: Option<FormId>,
    sights: bool,
    crouched: bool,
    blocking: bool,
}

#[derive(Resource, Default)]
pub struct Fighters {
    fighters: HashMap<u32, Fighter>,
    /// Each fighter's pose this frame.
    poses: HashMap<u32, Pose>,
    stats: Stats,
    rng: u64,
    /// The crouch, block and approach rolls' own sequence (§14), so fights without them
    /// roll as before.
    stance_rng: u64,
    /// The creatures' power attack rolls' own sequence (module notes).
    power_rng: u64,
    /// The combo swings' variant rolls' own sequence, as `stance_rng`.
    swing_rng: u64,
    /// The player's swings seen so far (`CombatStats::swings`), and when the last began.
    player_swings: u32,
    player_swing_at: f32,
}

impl Fighters {
    /// The pitch from a fighting actor's eyes to its target's chest, while it fights.
    pub fn pitch(&self, actor: u32) -> Option<f32> {
        self.poses.get(&actor).map(|p| p.pitch)
    }

    /// The weapon drawn on a fighting actor.
    pub fn weapon(&self, actor: u32) -> Option<FormId> {
        self.poses.get(&actor).and_then(|p| p.weapon)
    }

    /// Whether a fighting actor aims down its weapon's sights.
    pub fn sights(&self, actor: u32) -> bool {
        self.poses.get(&actor).is_some_and(|p| p.sights)
    }

    /// Whether a fighting actor is crouched to fire (docs/design/combat-ai.md §14.2).
    pub fn crouched(&self, actor: u32) -> bool {
        self.poses.get(&actor).is_some_and(|p| p.crouched)
    }

    /// Whether a fighting actor blocks (§14.3).
    pub fn blocking(&self, actor: u32) -> bool {
        self.poses.get(&actor).is_some_and(|p| p.blocking)
    }
}

#[derive(Default)]
struct Stats {
    fighters: BTreeMap<u32, String>,
    shots: usize,
    strikes: usize,
    reloads: usize,
    /// Blows whose target had moved out of reach by the time they landed.
    missed: usize,
    /// Shots from a weapon's muzzle, and from the eyes (no muzzle node).
    from_muzzle: usize,
    from_eyes: usize,
    /// Shots aimed down the sights and from the hip; times an actor raised its sights.
    sights_shots: usize,
    hip_shots: usize,
    sights_raised: usize,
    /// Melee decisions to hold instead of attacking; flee checks made and flights begun.
    holds: usize,
    /// Blows that went on a combo, and the longest combo.
    chained: usize,
    longest_combo: u32,
    flee_checks: usize,
    flees: usize,
    /// Hit reactions (crippled staggers) that held a fighter back, and blows being
    /// swung that they cut short.
    staggers: usize,
    blows_cut: usize,
    /// Frames in which a fighter that could have attacked waited to face its target.
    unfaced: usize,
    /// Crouching (§14.2): times crouched to fire, behind low cover; stood up, for a
    /// close-combat enemy; shifts.
    crouches: usize,
    low_crouches: usize,
    stands: usize,
    stands_close: usize,
    shifts: usize,
    /// Blocking (§14.3): blocks raised by decision and in reaction; blows blocked by
    /// people; recoils.
    blocks: usize,
    reactive_blocks: usize,
    blocked: usize,
    recoils: usize,
    /// Closing in (§14.4): bounds, charges in the open, guards begun.
    bounds: usize,
    charges: usize,
    guards: usize,
    /// Creatures' power attacks by attack group.
    powers: BTreeMap<&'static str, usize>,
}

/// The chance (percent) of a creature's blow being a power attack where its combat style
/// gives none (module notes).
pub const FALLBACK_POWER_CHANCE: f32 = 25.0;

/// Whether a creature's blow is a power attack, and which with its hit time (module
/// notes): `have`, the power attacks it has clips for; `u` and `v`, rolls in [0, 1).
/// Also says whether the chance was the fallback.
fn pick_power(style: Option<&fnv_game::data::CombatStyleDef>, have: &[(Power, f32)], u: f32, v: f32) -> (Option<(Power, f32)>, bool) {
    let own = style.map(|s| s.power_attack_chance).filter(|c| *c > 0.0);
    if have.is_empty() || u * 100.0 >= own.unwrap_or(FALLBACK_POWER_CHANCE) {
        return (None, own.is_none());
    }
    let share = |p: Power| style.map_or(0.0, |s| s.power_attack_dirs[if p == Power::Forward { 1 } else { 0 }].max(0.0));
    let total: f32 = have.iter().map(|h| share(h.0)).sum();
    if total <= 0.0 {
        return (Some(have[((v * have.len() as f32) as usize).min(have.len() - 1)]), own.is_none());
    }
    let mut x = v * total;
    for &h in have {
        x -= share(h.0);
        if x < 0.0 {
            return (Some(h), own.is_none());
        }
    }
    (have.last().copied(), own.is_none())
}

/// A number in [0, 1) from the module's own sequence `rng` [design].
pub(crate) fn next_unit(rng: &mut u64) -> f32 {
    *rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    (*rng >> 40) as f32 / (1u64 << 24) as f32
}

/// Damage per second of an attack, for the threat ratio [design: the documentation does
/// not say how the original rates it]: a gun's damage per shot (all its projectiles)
/// times its shots per second (its fire rate for automatic weapons, else its attack
/// shots per second); a blow's damage over [`STRIKE_SECONDS`] at the weapon's attack
/// speed.
fn dps(c: &Combat, arm: &Arm) -> f32 {
    let w = |id: Option<FormId>| id.or(c.fists_id()).and_then(|id| c.forms.weapons.get(&id));
    match arm {
        Arm::Gun(id) => w(Some(*id)).map_or(0.0, |ws| {
            let per_second = if ws.automatic && ws.fire_rate > 0.0 { ws.fire_rate } else if ws.attack_shots_per_sec > 0.0 { ws.attack_shots_per_sec } else { 1.0 };
            ws.base_damage.max(0) as f32 * per_second
        }),
        Arm::Melee(id) => w(*id).map_or(0.0, |ws| ws.base_damage.max(0) as f32 * ws.attack_mult.max(0.25) / STRIKE_SECONDS),
        Arm::Creature(d, _) => *d / STRIKE_SECONDS,
    }
}

/// The player's attack, for the threat ratio of those fighting the player: the weapon in
/// use, a gun or a blow.
fn player_arm(c: &Combat) -> Arm {
    match c.player_weapon() {
        Some(w) if c.forms.weapons.get(&w).is_some_and(|ws| is_gun(c, ws, w)) => Arm::Gun(w),
        w => Arm::Melee(w),
    }
}

/// How near an actor fighting with a melee reach or a gun's max range wants to be, and
/// how far its target may be before it closes in again (module notes): a gun stops at
/// [`GUN_RANGE_SHARE`] of its range and goes again beyond its range; melee stops at
/// [`MELEE_ENGAGE_SHARE`] of its reach and goes again beyond its reach [design].
pub fn engage(arm_reach: Option<f32>, gun_max_range: Option<f32>) -> (f32, f32) {
    match (gun_max_range, arm_reach) {
        (Some(r), _) if r > 0.0 => {
            let stop = (r * GUN_RANGE_SHARE).clamp(GUN_ENGAGE_MIN, GUN_ENGAGE_MAX);
            (stop, (stop / GUN_RANGE_SHARE).min(GUN_ENGAGE_MAX / GUN_RANGE_SHARE))
        }
        (Some(_), _) => (GUN_ENGAGE_MAX, GUN_ENGAGE_MAX / GUN_RANGE_SHARE),
        (None, reach) => {
            let reach = reach.unwrap_or(CREATURE_REACH);
            (reach * MELEE_ENGAGE_SHARE, reach)
        }
    }
}

/// Whether an actor fires down its sights at a target `distance` away (module notes):
/// only a gun with sights (`sight_fov` above 0), beyond [`HIP_FIRE_WITHIN`]; `was`: it
/// aimed down them last frame (they stay up to [`SIGHTS_HYSTERESIS`] nearer).
pub fn aims_down_sights(was: bool, distance: f32, sight_fov: f32) -> bool {
    sight_fov > 0.0 && distance > if was { HIP_FIRE_WITHIN - SIGHTS_HYSTERESIS } else { HIP_FIRE_WITHIN }
}

/// The extra spread of an actor that has fired at its target for `firing` seconds.
pub fn settling(firing: f32) -> f32 {
    SETTLE_DEGREES * (1.0 - firing / SETTLE_SECONDS).clamp(0.0, 1.0)
}

fn reach_of(c: &Combat, arm: &Arm, scale: f32) -> f32 {
    match arm {
        Arm::Gun(_) => 0.0,
        Arm::Melee(w) => w.or(c.fists_id()).and_then(|w| c.forms.weapons.get(&w)).map_or(CREATURE_REACH, |w| fnv_combat::damage::melee_reach(&c.rules, w, scale)),
        Arm::Creature(_, reach) => *reach,
    }
}

pub(crate) fn is_gun(c: &Combat, w: &WeaponStats, id: FormId) -> bool {
    AttackKind::of(w) == AttackKind::Gun && c.forms.chain(id, None).is_some_and(|ch| ch.projectile.is_some())
}

/// The navmesh, the player's place, the battle's counts, and who the AI moves.
type Where<'w> = (Option<Res<'w, NavState>>, Option<Res<'w, Location>>, ResMut<'w, Battle>, Option<Res<'w, crate::residents::Residents>>);

type Fought<'w> = (&'w ActorRig, &'w GlobalTransform, &'w InheritedVisibility, Option<&'w WeaponHold>, Has<Dying>, Has<Corpse>, Option<&'w Flinch>);

/// The share of its running speed a fighter keeps (`Awarenesses::pace`, read by the AI
/// that moves it): a two-handed weapon drawn costs `fMove2HBigPenalty` of it (item 61).
/// A creature's own running speed (its run clip's pace) is the AI's
/// (`fnv_ai::Ai::speeds`, set by `creature_pace.rs`); this was where creatures were held
/// below the people's running speed (the owner's playtest list 5 item 25).
fn pace(two_handed_drawn: bool, penalty: f32) -> f32 {
    if two_handed_drawn {
        (1.0 - penalty).clamp(0.1, 1.0)
    } else {
        1.0
    }
}

/// One frame of every fight (module notes).
#[allow(clippy::too_many_arguments)]
fn fight(
    time: Res<Time>,
    mut fighters: ResMut<Fighters>,
    mut combat: ResMut<Combat>,
    mut script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    player: Option<Res<Player>>,
    (blocks, mut phys): (Option<Res<ActorBlocks>>, Option<ResMut<PhysState>>),
    (nav, location, mut battle, residents): Where,
    actors: Query<Fought>,
    held: Query<&HeldWeapon>,
    muzzles: Query<(&Muzzle, &GlobalTransform)>,
    (mut fire, mut strike, mut anims): (MessageWriter<ActorFire>, MessageWriter<ActorStrike>, MessageWriter<PlayAttack>),
    mut times: ResMut<AttackTimes>,
    (cstats, control): (Option<Res<CombatStats>>, Option<Res<PlayerControl>>),
) {
    let dt = time.delta_secs();
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    if rt.game.menu.is_some() || rt.game.dialogue.is_some() {
        return;
    }
    // Blows blocked since last frame (docs/design/combat-ai.md §14.3).
    let blocked_blows: Vec<BlockedBlow> = std::mem::take(&mut combat.blocked_blows);
    // Who is crouched to fire: their eyes are lower (§14.2).
    let crouched_now: std::collections::HashSet<u32> = rt.game.awareness.crouching.iter().map(|a| a.0).collect();
    let mut guarding: HashMap<FormId, [f32; 2]> = HashMap::new();
    let c = &*combat;
    let weapons: HashMap<u32, FormId> = held.iter().map(|h| (h.reference, h.weapon)).collect();
    let muzzle_at: HashMap<u32, V3> = muzzles.iter().map(|(m, g)| (m.reference, bevy_to_game(g.translation()).to_array())).collect();
    // Where everyone is: feet and chest (game space), and size.
    let mut places: HashMap<u32, (V3, V3, f32)> = HashMap::new();
    for (rig, g, vis, _, dying, corpse, _) in &actors {
        if vis.get() && !dying && !corpse {
            let feet = bevy_to_game(g.translation()).to_array();
            let scale = g.affine().transform_vector3(Vec3::Z).length().max(0.1);
            let eye = EYE_HEIGHT * scale * if crouched_now.contains(&rig.reference) { stance::CROUCH_EYE } else { 1.0 };
            places.insert(rig.reference, (feet, [feet[0], feet[1], feet[2] + eye], scale));
        }
    }
    if let Some(p) = player.as_deref() {
        places.insert(FormId::PLAYER_REF.0, (p.feet, p.eye(), 1.0));
    }
    // The gap between an actor's body and its target's, when both are known.
    let player_radius = phys.as_deref().and_then(|p| p.player.as_ref()).map(|c| c.params.radius);
    let body = |id: u32| blocks.as_deref().and_then(|b| b.list.iter().find(|x| x.actor == id).cloned());
    let gap = |me: u32, target: u32, tfeet: V3| -> Option<f32> {
        let mine = body(me)?;
        let radius = if target == FormId::PLAYER_REF.0 { player_radius? } else { body(target)?.radius };
        Some(mine.room([tfeet[0], tfeet[1]], radius))
    };
    let f = &mut *fighters;
    f.poses.clear();
    if f.stance_rng == 0 {
        f.stance_rng = 0x2545_F491_4F6C_DD1D;
    }
    if f.power_rng == 0 {
        f.power_rng = 0xD1B5_4A32_D192_ED03;
    }
    if f.swing_rng == 0 {
        f.swing_rng = 0x9E37_79B9_7F4A_7C15;
    }
    let clock = time.elapsed_secs();
    let space = location.as_deref().map(|l| l.interior.map_or(Space::World(l.world), Space::Interior));
    let nav = nav.as_deref().map(|n| &n.graph);
    let mut still: Vec<u32> = Vec::new();
    // The player's swings (its blows, for those fighting it in close combat).
    let swings = cstats.as_deref().map_or(0, |s| s.swings);
    if swings > f.player_swings {
        f.player_swing_at = clock;
    }
    f.player_swings = swings;
    let player_close = matches!(player_arm(c), Arm::Melee(_));
    let player_blocking = player_close && control.as_deref().is_some_and(|k| k.aiming);
    with_world(rt, &mut sim, |rt| {
        let g = &mut rt.game;
        // Blocked blows: the attacker recoils, the blocker plays its block hit (§14.3).
        for b in &blocked_blows {
            if let Some(x) = f.fighters.get_mut(&b.attacker.0) {
                x.recoil_until = clock + stance::RECOIL_SECONDS;
                x.blows.clear();
                x.swing = None;
                f.stats.recoils += 1;
                info!("npc combat: {} recoils: its blow was blocked by {}", tactics::who(g, b.attacker), tactics::who(g, b.blocker));
                anims.write(PlayAttack { actor: b.attacker.0, weapon: weapons.get(&b.attacker.0).copied(), melee: true, sights: false, kind: Move::Recoil, power: None, swing: None });
            }
            if f.fighters.contains_key(&b.blocker.0) {
                f.stats.blocked += 1;
                anims.write(PlayAttack { actor: b.blocker.0, weapon: weapons.get(&b.blocker.0).copied(), melee: true, sights: false, kind: Move::BlockHit, power: None, swing: None });
            }
        }
        for (rig, global, _, hold, _, _, flinch) in &actors {
            let id = rig.reference;
            let me = FormId(id);
            let Some(&(feet, eyes, scale)) = places.get(&id) else { continue };
            // ToggleActorsAI or SetActorsAI 0: it does not fight.
            if g.ai_off(me) {
                continue;
            }
            // A script's weapon use comes first (`use_weapon`).
            if g.weapon_use(me).is_some_and(|w| w.source == fnv_game::use_weapon::Source::Script) {
                continue;
            }
            let Some(aw) = g.awareness.get(me).filter(|a| a.state == State::Combat) else { continue };
            // The target it detects; just after losing sight of it (still in Combat, not
            // yet Lost), the one detected last, so it keeps its distance to close in
            // (`fnv_ai::alert`) instead of standing where it cannot see [design: the
            // fiends at Cook-Cook's camp stood still behind a rise and went Lost and back
            // every few seconds].
            let latest = || aw.targets.iter().filter(|(_, t)| t.detected.is_some()).max_by(|x, y| x.1.detected.unwrap_or(0.0).total_cmp(&y.1.detected.unwrap_or(0.0))).map(|(id, t)| (*id, t));
            let Some((target, _)) = aw.combat_target().or_else(latest) else { continue };
            let Some(&(tfeet, teyes, _)) = places.get(&target.0) else { continue };
            still.push(id);
            // How fast it runs (`pace`).
            let two = crate::attack_anims::drawn(hold) && weapons.get(&id).and_then(|w| c.forms.weapons.get(w)).is_some_and(WeaponStats::two_handed);
            let p = pace(two, c.rules.move_2h_penalty);
            if g.awareness.pace_of(me) != p {
                info!(
                    "npc combat: {} {me} runs at {:.0}% of the running speed ({})",
                    g.data.name(me),
                    p * 100.0,
                    if two { "a two-handed weapon drawn" } else { "its own pace" }
                );
            }
            if p < 1.0 {
                g.awareness.pace.insert(me, p);
            } else {
                g.awareness.pace.remove(&me);
            }
            // What it fights with; its combat style may keep it to melee.
            let style = g.combat_style(me);
            let melee_only = style.is_some_and(|s| s.weapon_restrictions == 1);
            let arm = arm_of(c, g, weapons.get(&id).copied(), me, scale, melee_only);
            // Its strength, and its target's, for the threat ratios.
            let (health, full) = g.health_now_and_full(me);
            g.awareness.tactics.strengths.insert(me, Strength { health, dps: dps(c, &arm) as f64 });
            // What its target fights with: close combat or a gun (§14).
            let tarm = if target == FormId::PLAYER_REF { player_arm(c) } else { f.fighters.get(&target.0).map(|x| x.arm.clone()).unwrap_or_else(|| arm_of(c, g, weapons.get(&target.0).copied(), target, 1.0, false)) };
            let target_close = !matches!(tarm, Arm::Gun(_));
            // People only crouch, block and bound (owner 2026-10-16: "only affect human
            // enemies not creatures").
            let person = g.world.placed.get(me).is_some_and(|p| &p.sig == b"ACHR");
            if !f.fighters.contains_key(&target.0) || target == FormId::PLAYER_REF {
                let (th, _) = g.health_now_and_full(target);
                g.awareness.tactics.strengths.insert(target, Strength { health: th, dps: dps(c, &tarm) as f64 });
            }
            battle.side_of(g, id);
            battle.side_of(g, target.0);
            battle.fought(clock);
            // The gunmen of its group on this target (flank places), it among them.
            let mates: Vec<u32> = {
                let mut v: Vec<u32> = f
                    .fighters
                    .iter()
                    .filter(|(k, x)| **k != id && x.target == target && matches!(x.arm, Arm::Gun(_)) && g.comrades(me, FormId(**k)) && !g.awareness.tactics.flights.contains_key(&FormId(**k)))
                    .map(|(k, _)| *k)
                    .collect();
                v.push(id);
                v.sort();
                v
            };
            // Its opponent: blows begun, in a blow, blocking, recoiling, and when its blow on
            // the way lands (seconds).
            let (opp_blows, opp_attacking, opp_blocking, opp_recoiling, opp_hit_in) = if target == FormId::PLAYER_REF {
                (swings, clock - f.player_swing_at < stance::PLAYER_SWING_SECONDS, player_blocking, false, stance::PLAYER_SWING_HIT)
            } else {
                f.fighters.get(&target.0).map_or((0, false, false, false, DEFAULT_HIT), |t| {
                    (t.blows_begun, !t.blows.is_empty(), t.block.is_some(), t.recoil_until > clock, t.blows.iter().map(|b| b.0).fold(f32::MAX, f32::min).min(1.0))
                })
            };
            // Gunmen of its group with it in a fight, where they are (a melee person may
            // guard one, §14.4).
            let gun_friends: Vec<(u32, V3)> = if person && matches!(arm, Arm::Melee(_)) {
                f.fighters
                    .iter()
                    .filter(|(k, x)| **k != id && matches!(x.arm, Arm::Gun(_)) && g.comrades(me, FormId(**k)) && !g.awareness.tactics.flights.contains_key(&FormId(**k)))
                    .filter_map(|(k, _)| places.get(k).map(|p| (*k, p.0)))
                    .collect()
            } else {
                Vec::new()
            };
            let fresh = f.fighters.get(&id).is_none_or(|x| x.target != target || x.arm != arm);
            if fresh {
                let cycle = match &arm {
                    Arm::Gun(w) => c.forms.weapons.get(w).map(|ws| {
                        let timing = Timing::of(ws, &ClipTiming::default(), &c.tuning);
                        Cycle::new(ws, timing, false, ws.clip_size.max(1))
                    }),
                    _ => None,
                };
                let name = g.data.name(me);
                f.stats.fighters.insert(id, name.clone());
                info!("npc combat: {name} {me} fights {} with {}", if target == FormId::PLAYER_REF { "the player".to_string() } else { g.data.name(target) }, describe(c, &arm));
                // A new target keeps the fighter's tactics (its cover roll, its flight) and
                // its temperaments; a new fight draws them (§14.1).
                let (tactic, temper, approach) = f.fighters.remove(&id).map(|x| (x.tactic, Some(x.temper), Approach { charging: x.approach.charging, guarded: x.approach.guarded, ..Default::default() })).unwrap_or_default();
                let temper = temper.unwrap_or_else(|| Temperament { crouch: next_unit(&mut f.stance_rng), cover: next_unit(&mut f.stance_rng), guard: next_unit(&mut f.stance_rng) });
                let stance_in = stance::between(stance::STANCE_DECIDE, next_unit(&mut f.stance_rng));
                f.fighters.insert(
                    id,
                    Fighter {
                        target,
                        arm: arm.clone(),
                        seen_for: 0.0,
                        firing_for: 0.0,
                        cycle,
                        strike_in: 0.0,
                        pulled: false,
                        blows: Vec::new(),
                        sights: false,
                        ready_sights: false,
                        pull_in: 0.0,
                        confidence_mod: 0.0,
                        flee_check_in: 0.0,
                        tactic,
                        stagger_seen: None,
                        temper,
                        crouched_since: None,
                        still_for: 0.0,
                        last_feet: None,
                        stance_in,
                        unseen_for: 0.0,
                        block: None,
                        react: None,
                        seen_opp_blows: opp_blows,
                        blows_begun: 0,
                        recoil_until: 0.0,
                        approach,
                        swing: None,
                    },
                );
                // This fight's confidence modifier, not for the foolhardy; the first
                // flee check within the update time [documented: GECK wiki Confidence].
                let threshold = g.confidence_threshold(me);
                let (lo, hi) = g.confidence_modifier_range();
                let update = g.threat_ratio_update_time() as f32;
                let (u1, u2) = (next_unit(&mut f.rng), next_unit(&mut f.rng));
                let x = f.fighters.get_mut(&id).expect("inserted");
                x.confidence_mod = if threshold > 0.0 { lo + (hi - lo) * u1 as f64 } else { 0.0 };
                x.flee_check_in = update * u2;
            }
            // Its opponent is in the middle of a blow (the melee decision).
            let under_attack = opp_attacking;
            let fighter = f.fighters.get_mut(&id).expect("inserted");
            let temper = fighter.temper;
            // Confidence: once hurt, now and then, would it rather flee? At its flee spot,
            // attacked or cornered, does it stand and fight? (module notes)
            let away = (feet[0] - tfeet[0]).hypot(feet[1] - tfeet[1]);
            let sees_now = g.detection.get(me, target).is_some_and(|d| d.band == Band::Detected && d.seen);
            fighter.flee_check_in -= dt;
            let gs = g.group_settings();
            let target_name = tactics::who(g, target);
            let me_name = tactics::who(g, me);
            match g.awareness.tactics.flights.get(&me).copied() {
                Some(fl) => {
                    if let Some(arrived) = fl.arrived {
                        let at_spot = *fighter.tactic.fled_health.get_or_insert(health);
                        let hurt = health < at_spot - 0.5;
                        let cornered = sees_now && away <= gs.flee_boost_radius;
                        let waited = g.time - arrived >= gs.flee_wait;
                        if hurt || cornered || waited {
                            if hurt || cornered {
                                g.boost_confidence(me);
                                battle.boosts += 1;
                            }
                            fighter.tactic.fled_health = Some(health);
                            let check = g.flee_check(me, fighter.confidence_mod);
                            let why = if hurt { "hurt at its flee spot" } else if cornered { "cornered" } else { "waited at its flee spot" };
                            if !check.flees {
                                g.end_flight(me);
                                battle.back_to_fight += 1;
                                info!("npc combat: {me_name} turns to fight {target_name} again ({why}; confidence rating now {})", check.rating);
                            } else if hurt || cornered {
                                g.start_flight(me, target, tfeet);
                                info!("npc combat: {me_name} flees on from {target_name} ({why})");
                            } else if let Some(fl) = g.awareness.tactics.flights.get_mut(&me) {
                                fl.arrived = Some(g.time);
                            }
                        }
                    }
                }
                None if fighter.flee_check_in <= 0.0 => {
                    fighter.flee_check_in = g.threat_ratio_update_time() as f32 * next_unit(&mut f.rng);
                    if health < full {
                        f.stats.flee_checks += 1;
                        let check = g.flee_check(me, fighter.confidence_mod);
                        if check.flees {
                            g.start_flight(me, target, tfeet);
                            fighter.tactic.fled_health = None;
                            fighter.tactic.cover = None;
                            f.stats.flees += 1;
                            let side = battle.side_of(g, id);
                            battle.sides[side].flights += 1;
                            let (kind, ratio) = if check.personal { ("personal survival", check.personal_ratio) } else { ("group", check.group_ratio) };
                            info!(
                                "npc combat: {me_name} flees from {target_name}: {kind} threat ratio {:.3} below its confidence {:.4} (rating {}, modifier {:+.2}; health {health:.0} of {full:.0})",
                                ratio.unwrap_or(0.0),
                                check.threshold,
                                check.rating,
                                fighter.confidence_mod
                            );
                        }
                    }
                }
                None => {}
            }
            let flight = g.awareness.tactics.flights.get(&me).copied();
            let fleeing = flight.is_some();
            if let Some(fl) = flight {
                // How it fled, counted once: by its spot, or cornered.
                let key = (id, fl.since.to_bits());
                if !battle.counted_flights.contains(&key) {
                    match (fl.spot, fl.arrived) {
                        (Some((_, how)), _) => {
                            let k = match how {
                                fnv_game::combat_group::FleeBy::Door { .. } => 0,
                                fnv_game::combat_group::FleeBy::Cover => 1,
                                fnv_game::combat_group::FleeBy::Away => 2,
                            };
                            battle.flights[k] += 1;
                            battle.counted_flights.insert(key);
                        }
                        (None, Some(_)) => {
                            battle.cornered += 1;
                            battle.counted_flights.insert(key);
                        }
                        _ => {}
                    }
                }
            }
            // How near it wants to be.
            // The style's Ranged Weapon Max Range Mult scales the weapon's range.
            let range_mult = style.map_or(1.0, |s| if s.range_mult.1 > 0.0 { s.range_mult.1 } else { 1.0 });
            let gun = match &arm {
                Arm::Gun(w) => c.forms.weapons.get(w).map(|ws| ws.max_range * range_mult),
                _ => None,
            };
            let reach = reach_of(c, &arm, scale);
            let gap = gap(id, target.0, tfeet);
            let want = match (&arm, gap) {
                // Melee with bodies to measure: from where its feet are, as far as the
                // bodies' gap allows.
                (Arm::Melee(_) | Arm::Creature(..), Some(gap)) => {
                    let bodies = away - gap;
                    (bodies + MELEE_STOP_GAP, bodies + MELEE_HIT_GAP)
                }
                _ => engage((!matches!(arm, Arm::Gun(_))).then_some(reach), gun),
            };
            g.awareness.engage.insert(me, want);
            // Can it see its target?
            let sees = g.detection.get(me, target).is_some_and(|d| d.band == Band::Detected && d.seen);
            fighter.seen_for = if sees { fighter.seen_for + dt } else { 0.0 };
            let tchest = [tfeet[0], tfeet[1], tfeet[2] + (teyes[2] - tfeet[2]) * CHEST];
            // Tactics of a gunman (`combat_tactics`): cover, flank, line of fire.
            let mut hold_fire = fleeing;
            if matches!(arm, Arm::Gun(_)) && !fleeing {
                let name = &me_name;
                let t = &mut fighter.tactic;
                let mut want_post: Option<Post> = None;
                // A post the AI dropped (no way there): not that kind again for a while;
                // cover it could not reach is not looked at again.
                if let (Some(k), false) = (t.last_post, g.awareness.tactics.posts.contains_key(&me)) {
                    t.no_post = Some((k, clock + tactics::NO_POST_SECONDS));
                    if matches!(k, PostKind::Cover | PostKind::Peek) {
                        if let Some(cv) = t.cover.take() {
                            t.bad_cover.push(cv.behind);
                        }
                        t.cover_search_in = tactics::COVER_RETRY;
                    }
                }
                let barred = |t: &Tactic, k: PostKind| t.no_post.is_some_and(|(n, until)| n == k && clock < until);
                // Cover: rolled every 10 s by the style's Take Cover Chance.
                t.cover_roll_in -= dt;
                if t.cover_roll_in <= 0.0 {
                    t.cover_roll_in = tactics::COVER_EVERY;
                    let chance = style.map_or(0.0, |s| s.take_cover_chance);
                    t.use_cover = chance > 0.0 && next_unit(&mut f.rng) * 100.0 < chance;
                    // Failing that, a person's cover temperament gives it a second chance
                    // (§14.2), from the stance rolls' own sequence.
                    if !t.use_cover && person {
                        t.use_cover = next_unit(&mut f.stance_rng) * 100.0 < stance::COVER_BIAS * temper.cover;
                    }
                    if !t.use_cover && t.cover.take().is_some() {
                        info!("npc combat: {name} leaves its cover (no cover for the next {:.0} s)", tactics::COVER_EVERY);
                    }
                }
                if let Some(cv) = t.cover {
                    if !cv.edge.faces(tchest) || (cv.edge.mid[0] - tfeet[0]).hypot(cv.edge.mid[1] - tfeet[1]) > want.1 {
                        t.cover = None;
                        info!("npc combat: {name} gives up its cover: {target_name} is no longer behind it");
                    }
                }
                if t.use_cover && t.cover.is_none() {
                    t.cover_search_in -= dt;
                    if t.cover_search_in <= 0.0 {
                        t.cover_search_in = tactics::COVER_RETRY;
                        let taken: Vec<[f32; 3]> = g.awareness.tactics.posts.iter().filter(|(k, _)| **k != me).map(|(_, p)| p.at).chain(t.bad_cover.iter().copied()).collect();
                        if let (Some(nav), Some(space)) = (nav, space) {
                            let radius = style.map_or(0.0, |s| s.cover_search_radius);
                            let radius = if radius > 0.0 { radius } else { tactics::COVER_SEARCH_DEFAULT }.min(tactics::COVER_MAX_RUN);
                            // Standing eyes: a crouched fighter looks from where it would stand.
                            let eye = (eyes[2] - feet[2]) / if crouched_now.contains(&id) { stance::CROUCH_EYE } else { 1.0 };
                            if let Some(cv) = tactics::find_cover(nav, phys.as_deref_mut(), space, feet, tchest, eye, radius, want.1, &taken, person) {
                                battle.cover_taken += 1;
                                info!(
                                    "npc combat: {name} takes {}cover {:.0} units away, {:.0} units from {target_name} (a peek {:.0} units from it)",
                                    if cv.low { "low " } else { "" },
                                    (cv.behind[0] - feet[0]).hypot(cv.behind[1] - feet[1]),
                                    (cv.behind[0] - tfeet[0]).hypot(cv.behind[1] - tfeet[1]),
                                    (cv.peek[0] - cv.behind[0]).hypot(cv.peek[1] - cv.behind[1])
                                );
                                t.cover = Some(cv);
                            }
                        }
                    }
                }
                let timer = |rng: &mut u64, (lo, hi): (f32, f32)| clock + (lo + (hi - lo).max(0.0) * next_unit(rng)).max(0.5);
                if let Some(cv) = t.cover.as_mut() {
                    let near = |p: [f32; 3]| (p[0] - feet[0]).hypot(p[1] - feet[1]) < tactics::AT_POST;
                    let (wait, wait_fire, fire_t) = style.map_or(((2.0, 4.0), (1.0, 2.0), (2.0, 4.0)), |s| (s.wait_timer, s.wait_to_fire_timer, s.fire_timer));
                    match cv.phase {
                        Phase::Going => {
                            if near(cv.behind) {
                                cv.phase = Phase::Hiding { until: timer(&mut f.rng, wait) };
                            }
                        }
                        Phase::Hiding { until } => {
                            hold_fire = true;
                            if clock >= until {
                                cv.phase = Phase::WaitToFire { until: timer(&mut f.rng, wait_fire) };
                                battle.peeks += 1;
                            }
                        }
                        Phase::WaitToFire { until } => {
                            if sees_now && near(cv.peek) {
                                cv.phase = Phase::Firing { until: timer(&mut f.rng, fire_t) };
                            } else if clock >= until {
                                cv.phase = Phase::Hiding { until: timer(&mut f.rng, wait) };
                            }
                        }
                        Phase::Firing { until } => {
                            if clock >= until {
                                cv.phase = Phase::Hiding { until: timer(&mut f.rng, wait) };
                            }
                        }
                    }
                    let (at, kind) = match cv.phase {
                        Phase::Going | Phase::Hiding { .. } => (cv.behind, PostKind::Cover),
                        _ => (cv.peek, PostKind::Peek),
                    };
                    if !barred(t, kind) {
                        want_post = Some(Post { at, kind, run: true });
                    }
                }
                // A step back from a target too near, before any cover or flank: it is on top of it.
                {
                    let within = tactics::BACK_OFF_WITHIN;
                    let current = g.awareness.tactics.posts.get(&me).copied().filter(|p| p.kind == PostKind::BackOff);
                    let near = away < if current.is_some() { within + tactics::BACK_OFF_SLACK } else { within };
                    if near && !barred(t, PostKind::BackOff) {
                        // On to the step it is taking; reached, another one.
                        let at = match current.filter(|p| (p.at[0] - feet[0]).hypot(p.at[1] - feet[1]) >= tactics::AT_POST) {
                            Some(p) => p.at,
                            None => {
                                if current.is_none() {
                                    battle.back_offs += 1;
                                    info!("npc combat: {name} steps back from {target_name}, {away:.0} units away (within {within:.0})");
                                }
                                tactics::back_off(feet, tfeet, tactics::BACK_OFF_STEP)
                            }
                        };
                        want_post = Some(Post { at, kind: PostKind::BackOff, run: false });
                    }
                }
                // The line of fire: a comrade (or the player, when not its target) in the way.
                let blocker = places
                    .iter()
                    .filter(|(k, _)| **k != id && **k != target.0)
                    .map(|(k, p)| (*k, [p.0[0], p.0[1], p.0[2] + (p.1[2] - p.0[2]) * CHEST]))
                    .find(|(k, p)| tactics::in_line_of_fire(eyes, tchest, *p, tactics::LINE_OF_FIRE_WIDTH) && (*k == FormId::PLAYER_REF.0 || g.comrades(me, FormId(*k))) && !g.would_attack(me, FormId(*k)));
                match blocker {
                    Some((k, p)) => {
                        hold_fire = true;
                        if t.blocked_by != Some(k) {
                            t.blocked_by = Some(k);
                            battle.held_fire += 1;
                            info!("npc combat: {name} holds fire: {} stands in its line of fire to {target_name}", tactics::who(g, FormId(k)));
                        }
                        if want_post.is_none() && !barred(t, PostKind::Sidestep) {
                            let at = tactics::sidestep(feet, tfeet, p, tactics::SIDESTEP);
                            if g.awareness.tactics.posts.get(&me).is_none_or(|q| q.kind != PostKind::Sidestep) {
                                battle.sidesteps += 1;
                            }
                            want_post = Some(Post { at, kind: PostKind::Sidestep, run: false });
                        }
                    }
                    None => t.blocked_by = None,
                }
                // A shift sideways it decided on (§14.2), until it is there.
                if let Some(at) = t.shift {
                    if flat(at, feet) < tactics::AT_POST || barred(t, PostKind::Shift) || t.cover.is_some() {
                        t.shift = None;
                    } else if want_post.is_none() {
                        want_post = Some(Post { at, kind: PostKind::Shift, run: true });
                    }
                }
                // Flank places for a group's gunmen not in cover.
                if want_post.is_none() && mates.len() >= 2 && !barred(t, PostKind::Flank) {
                    t.flank_in -= dt;
                    let current = g.awareness.tactics.posts.get(&me).copied().filter(|p| p.kind == PostKind::Flank);
                    // Crouched to fire, it keeps the place it has (§14.2).
                    if crouched_now.contains(&id) {
                        want_post = current;
                    } else if t.flank_in <= 0.0 {
                        t.flank_in = tactics::FLANK_EVERY;
                        let index = mates.iter().position(|m| *m == id).unwrap_or(0);
                        let n = mates.iter().filter_map(|m| places.get(m)).count().max(1) as f32;
                        let centre = mates.iter().filter_map(|m| places.get(m)).fold([0.0f32; 3], |a, p| [a[0] + p.0[0] / n, a[1] + p.0[1] / n, a[2] + p.0[2] / n]);
                        let radius = away.min(want.0);
                        let at = tactics::flank_point(tfeet, centre, radius, tactics::flank_turn(index));
                        if (at[0] - feet[0]).hypot(at[1] - feet[1]) >= tactics::FLANK_MIN_MOVE {
                            if current.is_none_or(|c| (c.at[0] - at[0]).hypot(c.at[1] - at[1]) >= tactics::FLANK_MIN_MOVE) {
                                battle.flanks += 1;
                                info!("npc combat: {name} takes a flank place, {:.0}° round {target_name} from its group ({} gunmen)", tactics::flank_turn(index), mates.len());
                            }
                            want_post = Some(Post { at, kind: PostKind::Flank, run: true });
                        } else if let Some(c) = current {
                            want_post = Some(c);
                        }
                    } else {
                        want_post = current;
                    }
                }
                t.last_post = want_post.map(|p| p.kind);
                match want_post {
                    Some(p) => {
                        g.awareness.tactics.posts.insert(me, p);
                    }
                    None => {
                        g.awareness.tactics.posts.remove(&me);
                    }
                }
            } else if !fleeing && person && matches!(arm, Arm::Melee(_)) {
                // Closing in on a gunman: guard, bound or charge (§14.4).
                let ap = &mut fighter.approach;
                // A post the AI found no way to ends what it was for.
                if ap.posted.is_some() && !g.awareness.tactics.posts.contains_key(&me) {
                    match ap.posted {
                        Some(PostKind::Guard) => {
                            ap.guard = None;
                            ap.guarded = true;
                        }
                        _ => {
                            ap.bound = None;
                            ap.bound_in = stance::BOUND_RETRY;
                        }
                    }
                }
                let mut want_post: Option<Post> = None;
                if let Some(gd) = ap.guard.as_mut() {
                    let friend = gun_friends.iter().find(|(k, _)| *k == gd.friend).map(|x| x.1);
                    let check = stance::GuardCheck { me: feet, friend, target: tfeet, close_threat: target_close, guarded_for: clock - gd.since, patience: gd.patience };
                    match (stance::guard_release(&check), friend) {
                        (None, Some(fp)) => {
                            let at = stance::guard_point(fp, tfeet, gd.turn);
                            if flat(at, gd.at) > stance::GUARD_MOVE {
                                gd.at = at;
                            }
                            want_post = Some(Post { at: gd.at, kind: PostKind::Guard, run: true });
                        }
                        (why, _) => {
                            info!("npc combat: {me_name} leaves {} to go for {target_name}: {}", tactics::who(g, FormId(gd.friend)), why.unwrap_or("the gunman it guards is no longer fighting"));
                            ap.guard = None;
                            ap.guarded = true;
                        }
                    }
                } else if !ap.guarded && !target_close {
                    let pick = gun_friends.iter().filter(|(_, fp)| stance::may_guard(&temper, feet, *fp, tfeet)).min_by(|a, b| flat(a.1, feet).total_cmp(&flat(b.1, feet))).copied();
                    if let Some((friend, fp)) = pick {
                        let turn = (next_unit(&mut f.stance_rng) * 2.0 - 1.0) * stance::GUARD_TURN;
                        let patience = stance::between(stance::GUARD_PATIENCE, next_unit(&mut f.stance_rng));
                        let at = stance::guard_point(fp, tfeet, turn);
                        ap.guard = Some(Guarding { friend, since: clock, patience, turn, at });
                        f.stats.guards += 1;
                        info!("npc combat: {me_name} guards {} against {target_name} ({away:.0} units away; for up to {patience:.0} s)", tactics::who(g, FormId(friend)));
                        want_post = Some(Post { at, kind: PostKind::Guard, run: true });
                    }
                }
                if want_post.is_none() {
                    if target_close || away <= stance::CHARGE_WITHIN {
                        ap.bound = None;
                        ap.charging = false;
                    } else if let Some(b) = ap.bound.as_mut() {
                        if b.arrived.is_none() && flat(b.at, feet) < tactics::AT_POST {
                            b.arrived = Some(clock);
                        }
                        match b.arrived {
                            Some(t0) if clock >= t0 + b.pause => {
                                ap.bound = None;
                                ap.bound_in = 0.0;
                            }
                            _ => want_post = Some(Post { at: b.at, kind: PostKind::Advance, run: true }),
                        }
                    } else {
                        ap.bound_in -= dt;
                        if ap.bound_in <= 0.0 {
                            ap.bound_in = stance::BOUND_RETRY;
                            let chest = (eyes[2] - feet[2]) * CHEST;
                            let exposed = crate::detection::clear_sight(phys.as_deref_mut(), teyes, [feet[0], feet[1], feet[2] + chest]);
                            if exposed {
                                let taken: Vec<V3> = g.awareness.tactics.posts.iter().filter(|(k, _)| **k != me).map(|(_, p)| p.at).collect();
                                let pick = usize::from(next_unit(&mut f.stance_rng) < 0.5);
                                let found = match (nav, space) {
                                    (Some(nav), Some(space)) => tactics::find_bound(nav, phys.as_deref_mut(), space, feet, tfeet, teyes, chest, &taken, pick),
                                    _ => None,
                                };
                                match found {
                                    Some((at, gain)) => {
                                        let pause = stance::between(stance::BOUND_PAUSE, next_unit(&mut f.stance_rng));
                                        ap.bound = Some(Bound { at, arrived: None, pause });
                                        ap.charging = false;
                                        f.stats.bounds += 1;
                                        info!("npc combat: {me_name} bounds to cover {:.0} units away, {gain:.0} units nearer {target_name} ({away:.0} units away)", flat(at, feet));
                                        want_post = Some(Post { at, kind: PostKind::Advance, run: true });
                                    }
                                    None if !ap.charging => {
                                        ap.charging = true;
                                        f.stats.charges += 1;
                                        info!("npc combat: {me_name} charges {target_name} across open ground: no cover on the way ({away:.0} units away)");
                                    }
                                    None => {}
                                }
                            }
                        }
                    }
                }
                ap.posted = want_post.map(|p| p.kind);
                match want_post {
                    Some(p) => {
                        g.awareness.tactics.posts.insert(me, p);
                    }
                    None => {
                        g.awareness.tactics.posts.remove(&me);
                    }
                }
            } else if fleeing || !matches!(arm, Arm::Gun(_)) {
                g.awareness.tactics.posts.remove(&me);
            }
            // A hit reaction (the stagger of a crippled limb, `deaths.rs`) playing: no attack
            // until it has ended, and a blow being swung is cut short (owner's playtest
            // list 4 item 21) [design].
            let staggering = flinch.filter(|fl| clock < fl.at + fl.clip.clip.length());
            if let Some(fl) = staggering {
                if fighter.stagger_seen != Some(fl.at.to_bits()) {
                    fighter.stagger_seen = Some(fl.at.to_bits());
                    let cut = std::mem::take(&mut fighter.blows).len();
                    f.stats.staggers += 1;
                    f.stats.blows_cut += cut;
                    info!("npc combat: {me_name} holds its attacks while it staggers ({:.1} s){}", fl.at + fl.clip.clip.length() - clock, if cut > 0 { format!("; {cut} blow(s) cut short") } else { String::new() });
                }
            }
            // It attacks only facing its target (owner's playtest list 5 item 33: NPCs fired
            // while looking 90° away); the AI turns it (`fnv_ai::alert`).
            let off = facing_off(global, feet, tfeet);
            // Who the AI does not turn (no `--npc-packages`) fires as it stands; blows and
            // bites at close quarters are not held back (the item is about shooting).
            let facing = off <= FIRE_FACING || !matches!(arm, Arm::Gun(_)) || residents.as_deref().is_none_or(|r| !r.runs(id));
            if !facing && sees && fighter.seen_for >= REACTION_SECONDS {
                f.stats.unfaced += 1;
            }
            // Crouching to fire (§14.2): a person with a gun, by the rules of
            // `combat_stance`, deciding now and then.
            let mut crouched = g.awareness.crouching.contains(&me);
            let was = crouched;
            let mut stand_why: Option<&str> = None;
            if person && matches!(arm, Arm::Gun(_)) {
                let moved = fighter.last_feet.map_or(0.0, |l| flat(l, feet));
                fighter.last_feet = Some(feet);
                let settling = fighter.still_for < stance::CROUCH_SETTLE;
                fighter.still_for = if dt > 0.0 && moved / dt > stance::STILL_SPEED { 0.0 } else { fighter.still_for + dt };
                // Just settled somewhere (a post reached, a stop): it decides soon.
                if settling && fighter.still_for >= stance::CROUCH_SETTLE {
                    fighter.stance_in = fighter.stance_in.min(stance::between(stance::SETTLED_DECIDE, next_unit(&mut f.stance_rng)));
                }
                fighter.unseen_for = if sees { 0.0 } else { fighter.unseen_for + dt };
                let cover = fighter.tactic.cover;
                let post_pending = g.awareness.tactics.posts.get(&me).is_some_and(|p| flat(p.at, feet) > tactics::AT_POST);
                let low_hide = cover.is_some_and(|cv| cv.low && matches!(cv.phase, Phase::Hiding { .. }));
                let low_up = cover.is_some_and(|cv| cv.low && !matches!(cv.phase, Phase::Hiding { .. }));
                let s = stance::Situation { distance: away, unseen_for: fighter.unseen_for, close_threat: target_close, still_for: fighter.still_for, busy: fleeing || staggering.is_some() || post_pending };
                if low_hide && !post_pending && !fleeing {
                    if !crouched {
                        crouched = true;
                        f.stats.low_crouches += 1;
                    }
                } else if let Some(why) = stance::stand_reason(&s).or(low_up.then_some("to fire over its cover")) {
                    crouched = false;
                    stand_why = Some(why);
                } else {
                    fighter.stance_in -= dt;
                    if fighter.stance_in <= 0.0 {
                        fighter.stance_in = stance::between(stance::STANCE_DECIDE, next_unit(&mut f.stance_rng));
                        let u = next_unit(&mut f.stance_rng);
                        match fighter.crouched_since {
                            Some(since) if crouched => {
                                if u < stance::shift_chance(clock - since) {
                                    let step = stance::between(stance::SHIFT, next_unit(&mut f.stance_rng));
                                    let left = next_unit(&mut f.stance_rng) < 0.5;
                                    fighter.tactic.shift = Some(stance::shift_point(feet, tfeet, step, left));
                                    f.stats.shifts += 1;
                                    crouched = false;
                                    stand_why = Some("to shift");
                                    info!("npc combat: {me_name} shifts {step:.0} units to its {} after {:.1} s crouched", if left { "left" } else { "right" }, clock - since);
                                }
                            }
                            _ => {
                                if stance::may_crouch(&s) && u < stance::crouch_chance(&temper, away) {
                                    crouched = true;
                                }
                            }
                        }
                    }
                }
            } else {
                crouched = false;
            }
            if crouched != was {
                if crouched {
                    fighter.crouched_since = Some(clock);
                    f.stats.crouches += 1;
                    g.awareness.crouching.insert(me);
                    let low = fighter.tactic.cover.is_some_and(|cv| cv.low);
                    info!("npc combat: {me_name} crouches {} {target_name} ({away:.0} units away)", if low { "behind low cover from" } else { "to fire at" });
                } else {
                    fighter.crouched_since = None;
                    f.stats.stands += 1;
                    let why = stand_why.unwrap_or("it no longer fires");
                    if why == "a close-combat enemy is coming in" {
                        f.stats.stands_close += 1;
                    }
                    g.awareness.crouching.remove(&me);
                    info!("npc combat: {me_name} stands up ({why}; {target_name} {away:.0} units away)");
                }
            }
            // Blocking (§14.3): a melee person against a close-combat opponent; its
            // block ends; its opponent's blow begun may raise one after a reaction time.
            let can_block = person && target_close && !fleeing && staggering.is_none() && matches!(arm, Arm::Melee(_)) && fnv_actor::player_clips::blocks(melee_prefix(c, &arm));
            if !can_block {
                fighter.block = None;
                fighter.react = None;
            }
            if fighter.block.is_some_and(|(until, _)| clock >= until) {
                fighter.block = None;
            }
            if opp_blows > fighter.seen_opp_blows {
                fighter.seen_opp_blows = opp_blows;
                if can_block && fighter.blows.is_empty() && fighter.block.is_none() && fighter.react.is_none() && sees && off <= stance::BLOCK_REACT_FACING {
                    fighter.react = Some((clock + stance::between(stance::BLOCK_REACT, next_unit(&mut f.stance_rng)), clock + opp_hit_in));
                }
            }
            if let Some((due, lands)) = fighter.react.filter(|(due, _)| clock >= *due) {
                fighter.react = None;
                let chance = style.map_or(0.0, |s| melee_block_chance(s, true));
                if fighter.block.is_none() && next_unit(&mut f.stance_rng) * 100.0 < chance {
                    let until = (lands.max(clock) + stance::between(stance::BLOCK_AFTER, next_unit(&mut f.stance_rng))).max(clock + stance::BLOCK_MIN);
                    fighter.block = Some((until, true));
                    f.stats.blocks += 1;
                    f.stats.reactive_blocks += 1;
                    info!("npc combat: {me_name} blocks {target_name}'s blow (reacting, {:.2} s after it began; {chance:.0} %)", due - (lands - opp_hit_in));
                }
            }
            if fighter.block.is_some() {
                guarding.insert(me, heading(global));
            }
            let ready = sees
                && facing
                && !hold_fire
                && staggering.is_none()
                && fighter.block.is_none()
                && clock >= fighter.recoil_until
                && fighter.seen_for >= REACTION_SECONDS
                && hold.is_none_or(|h| h.anim.stage == Stage::Drawn);
            let pitch = (tchest[2] - eyes[2]).atan2((tchest[0] - eyes[0]).hypot(tchest[1] - eyes[1]));
            // Down the sights while it fires at a far target, from the hip near.
            let sight_fov = match &arm {
                Arm::Gun(w) => c.forms.weapons.get(w).map_or(0.0, |ws| ws.sight_fov),
                _ => 0.0,
            };
            let sights = ready && aims_down_sights(fighter.sights, away, sight_fov);
            if sights && !fighter.sights {
                f.stats.sights_raised += 1;
            }
            if sights != fighter.ready_sights && ready {
                fighter.ready_sights = sights;
                let name = g.data.name(me);
                if sights {
                    info!("npc combat: {name} {me} aims down the sights, its target {away:.0} units away (beyond {HIP_FIRE_WITHIN:.0}): walks while aiming");
                } else {
                    info!("npc combat: {name} {me} fires from the hip, its target {away:.0} units away (within {HIP_FIRE_WITHIN:.0})");
                }
            }
            fighter.sights = sights;
            if sights {
                g.awareness.aiming.insert(me);
            } else {
                g.awareness.aiming.remove(&me);
            }
            f.poses.insert(id, Pose { pitch, weapon: weapons.get(&id).copied(), sights, crouched, blocking: fighter.block.is_some() });
            match &arm {
                Arm::Gun(w) => {
                    let Some(ws) = c.forms.weapons.get(w) else { continue };
                    let Some(cycle) = fighter.cycle.as_mut() else { continue };
                    fighter.firing_for = if ready { fighter.firing_for + dt } else { 0.0 };
                    // Automatic weapons hold the trigger; others pull it again each time
                    // the cycle is ready and the style's firing delay has passed.
                    fighter.pull_in = (fighter.pull_in - dt).max(0.0);
                    let trigger = ready && (ws.automatic || (!fighter.pulled && fighter.pull_in <= 0.0));
                    fighter.pulled = trigger;
                    let events = cycle.step(&c.tuning, Input { trigger, reload: false, dt, carried: u32::MAX / 2 });
                    for e in events {
                        match e {
                            Event::Fired { .. } => {
                                if !ws.automatic {
                                    let mult = style.map_or((1.0, 1.0), |s| s.semi_auto_delay_mult);
                                    let (lo, hi) = (ws.semi_auto_fire_delay[0] * mult.0, ws.semi_auto_fire_delay[1] * mult.1);
                                    fighter.pull_in = (lo + (hi - lo).max(0.0) * next_unit(&mut f.rng)).max(0.0);
                                }
                                // Aimed from the eyes, drawn from the muzzle when the weapon
                                // has one (module notes).
                                let muzzle = muzzle_at.get(&id).copied();
                                let to = vec::sub(tchest, eyes);
                                let len = vec::len(to).max(1.0);
                                let aim = vec::scale(to, 1.0 / len);
                                let origin = vec::add(eyes, vec::scale(aim, MUZZLE_AHEAD));
                                let first = f.stats.from_muzzle + f.stats.from_eyes == 0 || fighter.firing_for <= dt;
                                match muzzle {
                                    Some(_) => f.stats.from_muzzle += 1,
                                    None => f.stats.from_eyes += 1,
                                }
                                if first {
                                    match muzzle {
                                        Some(m) => {
                                            let d = vec::sub(m, eyes);
                                            info!("npc combat: {me} shots are drawn from its weapon's muzzle ({:.0} units from its eyes, {:.0} below them)", vec::len(d), -d[2]);
                                        }
                                        None => info!("npc combat: {me} shots are drawn from its eyes (its weapon has no muzzle node)"),
                                    }
                                }
                                let skill = ws.skill.as_deref().and_then(fnv_game::functions::av_by_name).map_or(50.0, |code| fnv_game::functions::actor_value_capped(g, me, code)) as f32;
                                let strength = fnv_game::functions::actor_value_capped(g, me, fnv_game::actor_values::STRENGTH) as f32;
                                if fighter.sights {
                                    f.stats.sights_shots += 1;
                                } else {
                                    f.stats.hip_shots += 1;
                                }
                                // Crouched: no standing term in the spread (§14.2). A crippled
                                // arm (Left/Right Attack Condition) widens it as the player's
                                // [design: the same settings, actors.md §8.12].
                                let (left_arm_crippled, right_arm_crippled) = (g.is_crippled(me, 27), g.is_crippled(me, 28));
                                let st = Stance { aiming: fighter.sights, moving: false, running: false, sneaking: crouched, airborne: false, skill, strength, aiming_move_divisor: 1.0, left_arm_crippled, right_arm_crippled };
                                let cone = spread::cone_deg(spread::actor_spread(&c.rules, &c.tuning, ws, &st), 0.0, ws, &[]) + settling(fighter.firing_for);
                                fire.write(ActorFire { shooter: me, weapon: *w, origin, aim, cone, muzzle, harmless: false });
                                anims.write(PlayAttack { actor: id, weapon: Some(*w), melee: false, sights: fighter.sights, kind: Move::Attack, power: None, swing: None });
                                f.stats.shots += 1;
                                // Its Attack line, on its own timer (fnv_game::combat_lines).
                                g.attack_line(me, Some(target), false, false);
                            }
                            Event::ReloadStarted => f.stats.reloads += 1,
                            _ => {}
                        }
                    }
                }
                Arm::Melee(_) | Arm::Creature(..) => {
                    fighter.strike_in = (fighter.strike_in - dt).max(0.0);
                    let near = gap.map_or(away <= reach, |gap| gap <= MELEE_HIT_GAP);
                    // The last blow's clip, once started: its blow lands at its own `Hit`
                    // key, and the next decision comes at its `a:` key (module notes).
                    if let Some(sw) = fighter.swing.as_mut() {
                        sw.t += dt;
                        if let Some(clip) = times.swings.remove(&id).filter(|_| sw.clip.is_none()) {
                            sw.clip = Some(clip);
                            // (Before this frame's countdown below.)
                            if let Some(b) = fighter.blows.last_mut() {
                                b.0 = (clip.hit - sw.t + dt).max(0.0);
                            }
                            if let Some((at, _)) = clip.chain {
                                fighter.strike_in = fighter.strike_in.min((at - sw.t).max(0.0));
                            }
                        }
                        if sw.t > sw.clip.map_or(STRIKE_SECONDS * 2.0, |c| c.len) {
                            fighter.swing = None;
                        }
                    }
                    // Blows whose clip has reached its `Hit` key land, if the target is
                    // still within reach.
                    for b in &mut fighter.blows {
                        b.0 -= dt;
                    }
                    let (due, wait): (Vec<_>, Vec<_>) = std::mem::take(&mut fighter.blows).into_iter().partition(|b| b.0 <= 0.0);
                    fighter.blows = wait;
                    for (_, b) in due {
                        if near {
                            strike.write(ActorStrike { point: tchest, ..b });
                        } else {
                            f.stats.missed += 1;
                        }
                    }
                    // The melee decision: block (against close combat, §14.3), attack, or
                    // hold for the style's hold timer [documented: the order].
                    let decide = ready && near && (feet[2] - tfeet[2]).abs() < 128.0 && fighter.strike_in <= 0.0;
                    let mut blocks = false;
                    if decide && can_block {
                        let chance = style.map_or(0.0, |s| melee_block_chance(s, under_attack));
                        if next_unit(&mut f.stance_rng) * 100.0 < chance {
                            let held = stance::between(style.map_or((0.5, 1.5), |s| s.hold_timer), next_unit(&mut f.stance_rng)).max(stance::BLOCK_MIN);
                            fighter.block = Some((clock + held, false));
                            fighter.strike_in = held;
                            fighter.swing = None;
                            blocks = true;
                            f.stats.blocks += 1;
                            info!("npc combat: {me_name} blocks {target_name}'s blow (decided, {held:.1} s; {chance:.0} %)");
                            guarding.insert(me, heading(global));
                        }
                    }
                    let opponent = Opponent { attacking: under_attack, recoiling: opp_recoiling, blocking: opp_blocking };
                    let attack = decide
                        && !blocks
                        && style.is_none_or(|s| {
                            let (skill_name, h2h) = match &arm {
                                Arm::Melee(Some(w)) => (c.forms.weapons.get(w).and_then(|ws| ws.skill.clone()).unwrap_or_else(|| "Melee Weapons".into()), false),
                                Arm::Melee(None) => ("Unarmed".to_string(), true),
                                _ => ("Melee Weapons".to_string(), true),
                            };
                            let skill = fnv_game::functions::av_by_name(&skill_name).map_or(50.0, |code| fnv_game::functions::actor_value_capped(g, me, code)) as f32;
                            next_unit(&mut f.rng) * 100.0 < melee_attack_chance(s, skill, h2h, opponent)
                        });
                    if decide && !blocks && !attack {
                        let (lo, hi) = style.map_or((0.5, 1.5), |s| s.hold_timer);
                        fighter.strike_in = (lo + (hi - lo).max(0.0) * next_unit(&mut f.rng)).max(0.1);
                        f.stats.holds += 1;
                        fighter.swing = None;
                    }
                    if attack {
                        let speed = match &arm {
                            Arm::Melee(w) => w.or(c.fists_id()).and_then(|w| c.forms.weapons.get(&w)).map_or(1.0, |w| w.attack_mult.max(0.25)),
                            _ => 1.0,
                        };
                        fighter.strike_in = STRIKE_SECONDS / speed;
                        let dir = vec::normalize(vec::sub(tchest, eyes));
                        let (weapon, creature_damage) = match &arm {
                            Arm::Melee(w) => (*w, None),
                            Arm::Creature(d, _) => (None, Some(*d)),
                            Arm::Gun(_) => unreachable!(),
                        };
                        // A creature's power attack (module notes).
                        let power = match (&arm, times.powers.get(&id)) {
                            (Arm::Creature(..), Some(have)) if !have.is_empty() => {
                                let (u, v) = (next_unit(&mut f.power_rng), next_unit(&mut f.power_rng));
                                let (p, fallback) = pick_power(style, have, u, v);
                                if let Some((p, _)) = p {
                                    *f.stats.powers.entry(p.group()).or_default() += 1;
                                    let chance = style.map(|s| s.power_attack_chance).filter(|_| !fallback).unwrap_or(FALLBACK_POWER_CHANCE);
                                    info!("npc combat: {me_name} power attack {} at {target_name} ({chance:.0} %, {})", p.group(), if fallback { "fallback: its combat style gives none" } else { "from its combat style" });
                                }
                                p
                            }
                            _ => None,
                        };
                        // The blow lands at its clip's `Hit` key (the last clip's, as the
                        // next is chosen after this; a power attack's own) [design].
                        // Until its clip's own `Hit` key is known (above).
                        let delay = power.map(|p| p.1).or(times.hit.get(&id).copied()).unwrap_or(DEFAULT_HIT);
                        let anim = power.map(|p| p.0.group());
                        fighter.blows.push((delay, ActorStrike { attacker: me, target, weapon, creature_damage, point: tchest, dir, anim }));
                        // Going on a combo, or the first swing of one (module notes); a power attack
                        // plays its own clip and starts a new run [design].
                        let next = fighter.swing.filter(|_| power.is_none()).and_then(|s| s.next());
                        let run = next.map_or(1, |_| fighter.swing.map_or(0, |s| s.run) + 1);
                        if let Some(n) = next {
                            f.stats.chained += 1;
                            info!("npc combat: {me_name} goes on its combo with {n:?} (blow {run})");
                        }
                        f.stats.longest_combo = f.stats.longest_combo.max(run);
                        times.swings.remove(&id);
                        fighter.swing = Some(NpcSwing { t: 0.0, clip: None, run });
                        let swing = SwingPick { next, rolls: [next_unit(&mut f.swing_rng), next_unit(&mut f.swing_rng)] };
                        anims.write(PlayAttack { actor: id, weapon, melee: true, sights: false, kind: Move::Attack, power: power.map(|p| p.0), swing: power.is_none().then_some(swing) });
                        f.stats.strikes += 1;
                        fighter.blows_begun += 1;
                        g.attack_line(me, Some(target), false, false);
                    }
                }
            }
        }
        // Actors no longer fighting: no engage distance, no fighter, no post.
        let gone: Vec<u32> = f.fighters.keys().copied().filter(|id| !still.contains(id)).collect();
        for id in gone {
            f.fighters.remove(&id);
            g.awareness.engage.remove(&FormId(id));
            g.awareness.aiming.remove(&FormId(id));
            g.awareness.pace.remove(&FormId(id));
            g.awareness.tactics.posts.remove(&FormId(id));
            g.awareness.crouching.remove(&FormId(id));
        }
    });
    // Who blocks now, for the blows dealt this frame (`nvf_play::combat`).
    combat.guarding = guarding;
}

/// The stance prefix of a melee arm (`1hm`, `2hm`; fists `h2h`), for whether it blocks.
fn melee_prefix(c: &Combat, arm: &Arm) -> &'static str {
    match arm {
        Arm::Melee(Some(w)) => c.forms.weapons.get(w).and_then(|ws| ws.anim_type.as_deref()).and_then(fnv_actor::weapon::AnimType::from_name).map_or("h2h", fnv_actor::weapon::AnimType::prefix),
        Arm::Melee(None) => "h2h",
        _ => "",
    }
}

/// What an actor fights with: the weapon drawn on it (a gun, unless its style keeps it to
/// melee), else a creature's own attack, else its fists.
fn arm_of(c: &Combat, g: &fnv_game::game::Game, weapon: Option<FormId>, me: FormId, scale: f32, melee_only: bool) -> Arm {
    match weapon {
        Some(w) if melee_only && c.forms.weapons.get(&w).is_some_and(|ws| is_gun(c, ws, w)) => Arm::Melee(None),
        Some(w) if c.forms.weapons.get(&w).is_some_and(|ws| is_gun(c, ws, w)) => Arm::Gun(w),
        Some(w) => Arm::Melee(Some(w)),
        None => match g.creature_attack(me) {
            Some((damage, reach)) => Arm::Creature(damage as f32, reach.filter(|r| *r > 0.0).map_or(CREATURE_REACH, |r| r as f32) * scale),
            None => Arm::Melee(None),
        },
    }
}

fn describe(c: &Combat, arm: &Arm) -> String {
    let name = |w: &FormId| c.forms.weapons.get(w).and_then(|w| w.edid.clone()).unwrap_or_else(|| w.to_string());
    match arm {
        Arm::Gun(w) => format!("{} (gun)", name(w)),
        Arm::Melee(Some(w)) => format!("{} (melee)", name(w)),
        Arm::Melee(None) => "its fists".into(),
        Arm::Creature(d, r) => format!("its own attack ({d:.0} damage, reach {r:.0})"),
    }
}

fn report(mut exit: MessageReader<AppExit>, fighters: Res<Fighters>, stats: Option<Res<CombatStats>>) {
    if exit.read().next().is_none() {
        return;
    }
    let s = &fighters.stats;
    let (hits, damage, died) = stats.map_or((0, 0.0, false), |c| (c.player_hits, c.player_damage, c.player_died));
    info!(
        "npc combat summary: {} actors fought ({}); {} shots ({} from a muzzle, {} from the eyes; {} down the sights, {} from the hip, sights raised {} times), {} blows ({} went on a combo, the longest {}; {} missed: out of reach when they landed; {} melee decisions to hold), {} reloads; {} flee checks, {} fled; the player was hit {hits} times for {damage:.1}{}; {} staggers held a fighter's attacks ({} blows cut short); {} frames a fighter waited to face its target",
        s.fighters.len(),
        s.fighters.values().cloned().collect::<Vec<_>>().join(", "),
        s.shots,
        s.from_muzzle,
        s.from_eyes,
        s.sights_shots,
        s.hip_shots,
        s.sights_raised,
        s.strikes,
        s.chained,
        s.longest_combo,
        s.missed,
        s.holds,
        s.reloads,
        s.flee_checks,
        s.flees,
        if died { " and died" } else { "" },
        s.staggers,
        s.blows_cut,
        s.unfaced,
    );
    info!(
        "npc stance summary: crouched {} times ({} behind low cover), stood up {} times ({} for a close-combat enemy), {} shifts; {} blocks ({} in reaction), {} blows blocked by fighters, {} recoils; {} bounds to cover, {} charges in the open, {} guards",
        s.crouches,
        s.low_crouches,
        s.stands,
        s.stands_close,
        s.shifts,
        s.blocks,
        s.reactive_blocks,
        s.blocked,
        s.recoils,
        s.bounds,
        s.charges,
        s.guards,
    );
    if !s.powers.is_empty() {
        info!("npc power attacks summary: {}", s.powers.iter().map(|(g, n)| format!("{g} {n}")).collect::<Vec<_>>().join(", "));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_creatures_power_attack() {
        let both = [(Power::Normal, 0.4), (Power::Forward, 0.6)];
        // No style: the fallback chance, then evenly.
        assert_eq!(pick_power(None, &both, 0.1, 0.2), (Some((Power::Normal, 0.4)), true));
        assert_eq!(pick_power(None, &both, 0.1, 0.7), (Some((Power::Forward, 0.6)), true));
        assert_eq!(pick_power(None, &both, FALLBACK_POWER_CHANCE / 100.0, 0.7).0, None);
        assert_eq!(pick_power(None, &[], 0.0, 0.0).0, None);
        // The style's chance and shares.
        let s = fnv_game::data::CombatStyleDef { power_attack_chance: 50.0, power_attack_dirs: [0.0, 100.0, 0.0, 0.0, 0.0], ..Default::default() };
        assert_eq!(pick_power(Some(&s), &both, 0.45, 0.1), (Some((Power::Forward, 0.6)), false));
        assert_eq!(pick_power(Some(&s), &both, 0.55, 0.1).0, None);
        assert_eq!(pick_power(Some(&s), &both[..1], 0.1, 0.9).0, Some((Power::Normal, 0.4)));
    }

    #[test]
    fn a_combo_goes_on_between_the_a_key_and_the_clips_end() {
        use fnv_combat::melee::NextAttack;
        // `h2hattackright_a.kf`: Hit 0.30, a:L 0.63, end 1.43 [observed, fnv_combat::melee].
        let clip = crate::attack_anims::SwingClip { hit: 0.30, chain: Some((0.63, NextAttack::Left)), len: 1.43 };
        let at = |t| NpcSwing { t, clip: Some(clip), run: 1 }.next();
        assert_eq!(at(0.5), None);
        assert_eq!(at(0.63), Some(NextAttack::Left));
        assert_eq!(at(1.4), Some(NextAttack::Left));
        assert_eq!(at(1.5), None);
        // Before its clip started, or a clip without an `a:` key: a new run.
        assert_eq!(NpcSwing { t: 0.7, clip: None, run: 1 }.next(), None);
        assert_eq!(NpcSwing { t: 0.7, clip: Some(crate::attack_anims::SwingClip { chain: None, ..clip }), run: 1 }.next(), None);
    }

    #[test]
    fn two_handed_weapons_slow() {
        assert_eq!(pace(false, 0.1), 1.0);
        // A rifle drawn costs fMove2HBigPenalty.
        assert!((pace(true, 0.1) - 0.9).abs() < 1e-6);
    }

    #[test]
    fn guns_engage_within_their_range_and_melee_within_reach() {
        assert_eq!(engage(None, Some(1000.0)), (800.0, 1000.0));
        assert_eq!(engage(None, Some(100.0)).0, GUN_ENGAGE_MIN);
        assert_eq!(engage(None, Some(10_000.0)), (GUN_ENGAGE_MAX, GUN_ENGAGE_MAX / GUN_RANGE_SHARE));
        assert_eq!(engage(None, Some(0.0)).0, GUN_ENGAGE_MAX);
        let (stop, resume) = engage(Some(100.0), None);
        assert!((stop - 80.0).abs() < 1e-4 && (resume - 100.0).abs() < 1e-4);
    }

    #[test]
    fn sights_beyond_twenty_metres_hip_within() {
        assert!(!aims_down_sights(false, 1300.0, 55.0));
        assert!(aims_down_sights(false, 1500.0, 55.0));
        // Up, they stay up a little nearer; down, they come up only beyond 20 m.
        assert!(aims_down_sights(true, 1380.0, 55.0));
        assert!(!aims_down_sights(false, 1380.0, 55.0));
        assert!(!aims_down_sights(true, 1300.0, 55.0));
        // A weapon without sights never.
        assert!(!aims_down_sights(false, 3000.0, 0.0));
    }

    #[test]
    fn the_aim_settles() {
        assert_eq!(settling(0.0), SETTLE_DEGREES);
        assert!((settling(SETTLE_SECONDS / 2.0) - SETTLE_DEGREES / 2.0).abs() < 1e-5);
        assert_eq!(settling(10.0), 0.0);
    }
}
