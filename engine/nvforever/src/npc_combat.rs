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
//!   first saw it in this fight, with its weapon drawn. A gun keeps its own cycle
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
//! - **Confidence** (`fnv_game::combat_style`): once hurt, every random 0 to
//!   fCombatThreatRatioUpdateTime seconds it compares its threat ratio (its health ×
//!   damage per second over its target's) with its confidence threshold and this
//!   fight's confidence modifier; fleeing is not built, so a decision to flee is logged
//!   (`would flee`) and counted.
//!
//! Whom it fights is the detection states' (its combat target); this module only
//! attacks. Not built: cover, fleeing, grenades, ammunition use, weapon choice among
//! several, blocking, dodging and power attacks of NPCs.

use crate::attack_anims::{AttackTimes, PlayAttack, DEFAULT_HIT};
use crate::limbs::ActorRig;
use crate::locomotion::Dying;
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
use fnv_game::combat_style::{flees, melee_attack_chance, threat_ratio};
use fnv_game::detection::Band;
use fnv_sim::FormId;
use nvf_play::blocking::ActorBlocks;
use nvf_play::combat::{ActorFire, ActorStrike, Combat, CombatStats};
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
const MUZZLE_AHEAD: f32 = 24.0;
/// The chest as a fraction of the eye height (as `detection.rs` aims its sight).
const CHEST: f32 = 0.75;

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
    /// Aiming down its sights now.
    sights: bool,
    /// Seconds before a semi-automatic gun may be fired again (its combat style's
    /// firing delay).
    pull_in: f32,
    /// This fight's confidence modifier; seconds to the next flee check; whether it
    /// has decided it would flee (logged once).
    confidence_mod: f64,
    flee_check_in: f32,
    would_flee: bool,
}

#[derive(Resource, Default)]
pub struct Fighters {
    fighters: HashMap<u32, Fighter>,
    /// Each fighter's pitch to its target (radians, negative looks down), the weapon
    /// drawn on it and whether it aims down the sights, this frame.
    poses: HashMap<u32, (f32, Option<FormId>, bool)>,
    stats: Stats,
    rng: u64,
}

impl Fighters {
    /// The pitch from a fighting actor's eyes to its target's chest, while it fights.
    pub fn pitch(&self, actor: u32) -> Option<f32> {
        self.poses.get(&actor).map(|p| p.0)
    }

    /// The weapon drawn on a fighting actor.
    pub fn weapon(&self, actor: u32) -> Option<FormId> {
        self.poses.get(&actor).and_then(|p| p.1)
    }

    /// Whether a fighting actor aims down its weapon's sights.
    pub fn sights(&self, actor: u32) -> bool {
        self.poses.get(&actor).is_some_and(|p| p.2)
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
    /// Melee decisions to hold instead of attacking; flee checks made and decisions to
    /// flee (not acted on).
    holds: usize,
    flee_checks: usize,
    would_flee: usize,
}

/// A number in [0, 1) from the module's own sequence `rng` [design].
fn next_unit(rng: &mut u64) -> f32 {
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

fn is_gun(c: &Combat, w: &WeaponStats, id: FormId) -> bool {
    AttackKind::of(w) == AttackKind::Gun && c.forms.chain(id, None).is_some_and(|ch| ch.projectile.is_some())
}

type Fought<'w> = (&'w ActorRig, &'w GlobalTransform, &'w InheritedVisibility, Option<&'w WeaponHold>, Has<Dying>, Has<Corpse>);

/// One frame of every fight (module notes).
#[allow(clippy::too_many_arguments)]
fn fight(
    time: Res<Time>,
    mut fighters: ResMut<Fighters>,
    combat: Res<Combat>,
    mut script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    player: Option<Res<Player>>,
    (blocks, phys): (Option<Res<ActorBlocks>>, Option<Res<PhysState>>),
    actors: Query<Fought>,
    held: Query<&HeldWeapon>,
    muzzles: Query<(&Muzzle, &GlobalTransform)>,
    (mut fire, mut strike, mut anims): (MessageWriter<ActorFire>, MessageWriter<ActorStrike>, MessageWriter<PlayAttack>),
    times: Res<AttackTimes>,
) {
    let dt = time.delta_secs();
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    if rt.game.menu.is_some() || rt.game.dialogue.is_some() {
        return;
    }
    let c = &*combat;
    let weapons: HashMap<u32, FormId> = held.iter().map(|h| (h.reference, h.weapon)).collect();
    let muzzle_at: HashMap<u32, V3> = muzzles.iter().map(|(m, g)| (m.reference, bevy_to_game(g.translation()).to_array())).collect();
    // Where everyone is: feet and chest (game space), and size.
    let mut places: HashMap<u32, (V3, V3, f32)> = HashMap::new();
    for (rig, g, vis, _, dying, corpse) in &actors {
        if vis.get() && !dying && !corpse {
            let feet = bevy_to_game(g.translation()).to_array();
            let scale = g.affine().transform_vector3(Vec3::Z).length().max(0.1);
            places.insert(rig.reference, (feet, [feet[0], feet[1], feet[2] + EYE_HEIGHT * scale], scale));
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
    // Each fighter's attack, for the threat ratio of whoever fights it.
    let mut arms: HashMap<u32, Arm> = f.fighters.iter().map(|(id, x)| (*id, x.arm.clone())).collect();
    arms.insert(FormId::PLAYER_REF.0, player_arm(c));
    let mut still: Vec<u32> = Vec::new();
    with_world(rt, &mut sim, |rt| {
        let g = &mut rt.game;
        for (rig, _, _, hold, _, _) in &actors {
            let id = rig.reference;
            let me = FormId(id);
            let Some(&(feet, eyes, scale)) = places.get(&id) else { continue };
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
            // What it fights with; its combat style may keep it to melee.
            let style = g.combat_style(me);
            let melee_only = style.is_some_and(|s| s.weapon_restrictions == 1);
            let arm = match weapons.get(&id).copied() {
                Some(w) if melee_only && c.forms.weapons.get(&w).is_some_and(|ws| is_gun(c, ws, w)) => Arm::Melee(None),
                Some(w) if c.forms.weapons.get(&w).is_some_and(|ws| is_gun(c, ws, w)) => Arm::Gun(w),
                Some(w) => Arm::Melee(Some(w)),
                None => match g.creature_attack(me) {
                    Some((damage, reach)) => Arm::Creature(damage as f32, reach.filter(|r| *r > 0.0).map_or(CREATURE_REACH, |r| r as f32) * scale),
                    None => Arm::Melee(None),
                },
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
                f.fighters.insert(id, Fighter { target, arm: arm.clone(), seen_for: 0.0, firing_for: 0.0, cycle, strike_in: 0.0, pulled: false, blows: Vec::new(), sights: false, pull_in: 0.0, confidence_mod: 0.0, flee_check_in: 0.0, would_flee: false });
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
            let under_attack = f.fighters.get(&target.0).is_some_and(|t| !t.blows.is_empty());
            let fighter = f.fighters.get_mut(&id).expect("inserted");
            // Confidence: once hurt, now and then, would it rather flee? (module notes)
            fighter.flee_check_in -= dt;
            if fighter.flee_check_in <= 0.0 && !fighter.would_flee {
                fighter.flee_check_in = g.threat_ratio_update_time() as f32 * next_unit(&mut f.rng);
                let (health, full) = g.health_now_and_full(me);
                if health < full {
                    f.stats.flee_checks += 1;
                    let (their_health, _) = g.health_now_and_full(target);
                    let theirs = arms.get(&target.0).map_or(0.0, |a| dps(c, a));
                    let ratio = threat_ratio(health, dps(c, &arm) as f64, their_health, theirs as f64);
                    let threshold = g.confidence_threshold(me);
                    if flees(ratio, threshold, fighter.confidence_mod) {
                        fighter.would_flee = true;
                        f.stats.would_flee += 1;
                        info!(
                            "npc combat: {} {me} would flee (threat ratio {ratio:.3} below its confidence {threshold:.4}, modifier {:+.2}; fleeing is not built)",
                            g.data.name(me),
                            fighter.confidence_mod
                        );
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
            let away = (feet[0] - tfeet[0]).hypot(feet[1] - tfeet[1]);
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
            let ready = sees && fighter.seen_for >= REACTION_SECONDS && hold.is_none_or(|h| h.anim.stage == Stage::Drawn);
            let tchest = [tfeet[0], tfeet[1], tfeet[2] + (teyes[2] - tfeet[2]) * CHEST];
            let pitch = (tchest[2] - eyes[2]).atan2((tchest[0] - eyes[0]).hypot(tchest[1] - eyes[1]));
            // Down the sights while it fires at a far target, from the hip near.
            let sight_fov = match &arm {
                Arm::Gun(w) => c.forms.weapons.get(w).map_or(0.0, |ws| ws.sight_fov),
                _ => 0.0,
            };
            let sights = ready && aims_down_sights(fighter.sights, away, sight_fov);
            if sights != fighter.sights && ready {
                let name = g.data.name(me);
                if sights {
                    f.stats.sights_raised += 1;
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
            f.poses.insert(id, (pitch, weapons.get(&id).copied(), sights));
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
                                let stance = Stance { aiming: fighter.sights, moving: false, running: false, sneaking: false, airborne: false, skill, strength, aiming_move_divisor: 1.0, left_arm_crippled: false, right_arm_crippled: false };
                                let cone = spread::cone_deg(spread::actor_spread(&c.rules, &c.tuning, ws, &stance), 0.0, ws, &[]) + settling(fighter.firing_for);
                                fire.write(ActorFire { shooter: me, weapon: *w, origin, aim, cone, muzzle });
                                anims.write(PlayAttack { actor: id, weapon: Some(*w), melee: false, sights: fighter.sights });
                                f.stats.shots += 1;
                            }
                            Event::ReloadStarted => f.stats.reloads += 1,
                            _ => {}
                        }
                    }
                }
                Arm::Melee(_) | Arm::Creature(..) => {
                    fighter.strike_in = (fighter.strike_in - dt).max(0.0);
                    let near = gap.map_or(away <= reach, |gap| gap <= MELEE_HIT_GAP);
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
                    // The melee decision: attack, or hold for the style's hold timer.
                    let decide = ready && near && (feet[2] - tfeet[2]).abs() < 128.0 && fighter.strike_in <= 0.0;
                    let attack = decide
                        && style.is_none_or(|s| {
                            let (skill_name, h2h) = match &arm {
                                Arm::Melee(Some(w)) => (c.forms.weapons.get(w).and_then(|ws| ws.skill.clone()).unwrap_or_else(|| "Melee Weapons".into()), false),
                                Arm::Melee(None) => ("Unarmed".to_string(), true),
                                _ => ("Melee Weapons".to_string(), true),
                            };
                            let skill = fnv_game::functions::av_by_name(&skill_name).map_or(50.0, |code| fnv_game::functions::actor_value_capped(g, me, code)) as f32;
                            next_unit(&mut f.rng) * 100.0 < melee_attack_chance(s, skill, h2h, under_attack)
                        });
                    if decide && !attack {
                        let (lo, hi) = style.map_or((0.5, 1.5), |s| s.hold_timer);
                        fighter.strike_in = (lo + (hi - lo).max(0.0) * next_unit(&mut f.rng)).max(0.1);
                        f.stats.holds += 1;
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
                        // The blow lands at its clip's `Hit` key (the last clip's, as the
                        // next is chosen after this) [design].
                        let delay = times.hit.get(&id).copied().unwrap_or(DEFAULT_HIT);
                        fighter.blows.push((delay, ActorStrike { attacker: me, target, weapon, creature_damage, point: tchest, dir }));
                        anims.write(PlayAttack { actor: id, weapon, melee: true, sights: false });
                        f.stats.strikes += 1;
                    }
                }
            }
        }
        // Actors no longer fighting: no engage distance, no fighter.
        let gone: Vec<u32> = f.fighters.keys().copied().filter(|id| !still.contains(id)).collect();
        for id in gone {
            f.fighters.remove(&id);
            g.awareness.engage.remove(&FormId(id));
            g.awareness.aiming.remove(&FormId(id));
        }
    });
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
        "npc combat summary: {} actors fought ({}); {} shots ({} from a muzzle, {} from the eyes; {} down the sights, {} from the hip, sights raised {} times), {} blows ({} missed: out of reach when they landed; {} melee decisions to hold), {} reloads; {} flee checks, {} would flee; the player was hit {hits} times for {damage:.1}{}",
        s.fighters.len(),
        s.fighters.values().cloned().collect::<Vec<_>>().join(", "),
        s.shots,
        s.from_muzzle,
        s.from_eyes,
        s.sights_shots,
        s.hip_shots,
        s.sights_raised,
        s.strikes,
        s.missed,
        s.holds,
        s.reloads,
        s.flee_checks,
        s.would_flee,
        if died { " and died" } else { "" },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

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
