//! Scripted weapon use (docs/design/use-weapon.md): an actor with a weapon use
//! (`fnv_game::use_weapon`: the UseWeapon function, a Use Weapon package) that the AI has
//! brought to its place and turned to its target fires its shots at it, as the combat
//! AI's gunmen do (`npc_combat`: the weapon's own cycle, aimed from the eyes at the
//! target's chest, drawn from the muzzle), counting them on the game's use.
//!
//! - **When it fires**: with its weapon drawn (`alert.rs` draws it for a weapon use),
//!   facing its target (within `npc_combat::FIRE_FACING`), not restrained (SetRestrained
//!   "blocks weapon use from Use Weapon packages and UseWeapon" [documented: catalogue
//!   notes on SetRestrained]), its target where the use wants it (the function's last
//!   argument, within `fnv_game::use_weapon::TARGET_HERE_RADIUS`), and, with Hold Fire
//!   When Blocked, nobody else standing in its line of fire (`combat_tactics`) [design:
//!   what "blocked" covers is not documented; walls are not looked at]. Between volleys
//!   it waits its pause.
//! - **The weapon**: the use's weapon when it is a gun, else the gun drawn on it; with
//!   neither the use ends [design]. Its drawn model is the one it was spawned with: a use
//!   naming another weapon fires that weapon's shots from the model it holds (models are
//!   not swapped live yet).
//! - **Always Hit**: no spread at all, aimed straight at the target's chest [design: the
//!   shot can still meet cover between them]. **Do No Damage**: the shots are harmless
//!   (`nvf_play::combat::ActorFire::harmless`). **Crouch To Reload**: it crouches
//!   (`Game::sneaks`) while its weapon reloads.
//! - **The end**: all shots fired, its target dead or gone, or no gun: the game ends the
//!   use (`Game::end_weapon_use`).

use crate::attack_anims::PlayAttack;
use crate::combat_tactics as tactics;
use crate::limbs::ActorRig;
use crate::locomotion::Dying;
use crate::npc_combat::{facing_off, is_gun, next_unit, CHEST, FIRE_FACING, MUZZLE_AHEAD};
use crate::ragdoll::Corpse;
use crate::weapons::{HeldWeapon, Muzzle, WeaponHold};
use crate::world::bevy_to_game;
use bevy::prelude::*;
use fnv_actor::weapon::Stage;
use fnv_combat::attack::{ClipTiming, Cycle, Event, Input, Timing};
use fnv_combat::spread::{self, Stance};
use fnv_combat::vec::{self, V3};
use fnv_game::use_weapon::{After, Source, TARGET_HERE_RADIUS};
use fnv_sim::FormId;
use nvf_play::combat::{ActorFire, Combat, CombatStats};
use nvf_play::player::{Player, EYE_HEIGHT};
use nvf_play::script::{with_world, ScriptState};
use nvf_play::sim::SimState;
use std::collections::{BTreeMap, HashMap};

pub struct UseWeaponPlugin;

impl Plugin for UseWeaponPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Shooters>()
            .add_systems(Update, shoot.after(nvf_play::PlaySet::Scripts).run_if(resource_exists::<Combat>))
            .add_systems(Last, report);
    }
}

/// One actor carrying out a weapon use.
struct Shooter {
    /// The use's number (`WeaponUse::seq`) and the gun it fires.
    seq: u64,
    weapon: FormId,
    cycle: Cycle,
    pulled: bool,
    /// Seconds before it may pull the trigger again: a semi-automatic gun's delay, or
    /// the pause between volleys.
    wait: f32,
    /// What it last waited for, told once each time it changes.
    held: Option<&'static str>,
}

#[derive(Resource, Default)]
pub struct Shooters {
    by: HashMap<u32, Shooter>,
    stats: Stats,
    rng: u64,
}

#[derive(Default)]
struct Stats {
    actors: BTreeMap<u32, String>,
    from_scripts: usize,
    from_packages: usize,
    done: usize,
    shots: usize,
    sure: usize,
    harmless: usize,
    reloads: usize,
    crouched: usize,
    /// Times it held fire: someone in its line of fire, its target not where the use
    /// wants it, restrained.
    blocked: usize,
    target_away: usize,
    restrained: usize,
}

type Shooting<'w> = (&'w ActorRig, &'w GlobalTransform, &'w InheritedVisibility, Option<&'w WeaponHold>, Has<Dying>, Has<Corpse>);

#[allow(clippy::too_many_arguments)]
fn shoot(
    time: Res<Time>,
    mut shooters: ResMut<Shooters>,
    combat: Res<Combat>,
    mut script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    player: Option<Res<Player>>,
    actors: Query<Shooting>,
    held: Query<&HeldWeapon>,
    muzzles: Query<(&Muzzle, &GlobalTransform)>,
    (mut fire, mut anims): (MessageWriter<ActorFire>, MessageWriter<PlayAttack>),
) {
    let dt = time.delta_secs();
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    if rt.game.menu.is_some() || rt.game.dialogue.is_some() {
        return;
    }
    let c = &*combat;
    let weapons: HashMap<u32, FormId> = held.iter().map(|h| (h.reference, h.weapon)).collect();
    let muzzle_at: HashMap<u32, V3> = muzzles.iter().map(|(m, g)| (m.reference, bevy_to_game(g.translation()).to_array())).collect();
    // Feet and eyes (game space) of everyone standing.
    let mut places: HashMap<u32, (V3, V3)> = HashMap::new();
    for (rig, g, vis, _, dying, corpse) in &actors {
        if vis.get() && !dying && !corpse {
            let feet = bevy_to_game(g.translation()).to_array();
            let scale = g.affine().transform_vector3(Vec3::Z).length().max(0.1);
            places.insert(rig.reference, (feet, [feet[0], feet[1], feet[2] + EYE_HEIGHT * scale]));
        }
    }
    if let Some(p) = player.as_deref() {
        places.insert(FormId::PLAYER_REF.0, (p.feet, p.eye()));
    }
    let s = &mut *shooters;
    let mut still: Vec<u32> = Vec::new();
    with_world(rt, &mut sim, |rt| {
        let g = &mut rt.game;
        for (rig, global, _, hold, _, _) in &actors {
            let id = rig.reference;
            let me = FormId(id);
            if g.ai_off(me) {
                continue;
            }
            let Some(&(feet, eyes)) = places.get(&id) else { continue };
            let Some(w) = g.active_weapon_use(me).filter(|w| w.in_place).cloned() else { continue };
            let name = g.data.name(me);
            let target_name = if w.target == FormId::PLAYER_REF { "the player".to_string() } else { g.data.name(w.target) };
            // Its target dead or gone: the use is over [design].
            if g.extra.get(&w.target).is_some_and(|e| e.dead) || !g.present(w.target) {
                info!("use weapon: {name} {me} stops: {target_name} is dead or gone ({} shots fired)", w.fired);
                g.end_weapon_use(me);
                s.stats.done += 1;
                continue;
            }
            let Some(&(tfeet, teyes)) = places.get(&w.target.0) else { continue };
            // The gun: the use's, else the one drawn on it.
            let gun = w.weapon.into_iter().chain(weapons.get(&id).copied()).find(|f| c.forms.weapons.get(f).is_some_and(|ws| is_gun(c, ws, *f)));
            let Some(gun) = gun else {
                info!("use weapon: {name} {me} has no gun to use on {target_name}: the use ends");
                g.end_weapon_use(me);
                s.stats.done += 1;
                continue;
            };
            let Some(ws) = c.forms.weapons.get(&gun) else { continue };
            still.push(id);
            let fresh = s.by.get(&id).is_none_or(|x| x.seq != w.seq || x.weapon != gun);
            if fresh {
                let timing = Timing::of(ws, &ClipTiming::default(), &c.tuning);
                s.by.insert(id, Shooter { seq: w.seq, weapon: gun, cycle: Cycle::new(ws, timing, false, ws.clip_size.max(1)), pulled: false, wait: 0.0, held: None });
                s.stats.actors.insert(id, name.clone());
                match w.source {
                    Source::Script => s.stats.from_scripts += 1,
                    Source::Package(_) => s.stats.from_packages += 1,
                }
                let shots = match w.volleys {
                    Some(v) if w.shots_per_volley[0] == w.shots_per_volley[1] => format!("{} shots", v * w.shots_per_volley[0]),
                    Some(v) => format!("{v} volleys of {} to {} shots", w.shots_per_volley[0], w.shots_per_volley[1]),
                    None => format!("volleys of {} to {} shots, on and on", w.shots_per_volley[0], w.shots_per_volley[1]),
                };
                let flags: Vec<&str> = [(w.always_hit, "always hit"), (w.no_damage, "no damage"), (w.crouch_to_reload, "crouch to reload"), (w.hold_fire_when_blocked, "hold fire when blocked")].into_iter().filter(|f| f.0).map(|f| f.1).collect();
                let from = match w.source {
                    Source::Script => "a script".to_string(),
                    Source::Package(p) => format!("its package {}", g.data.name(p)),
                };
                let weapon_name = ws.edid.clone().unwrap_or_else(|| gun.to_string());
                info!(
                    "use weapon: {name} {me} uses {weapon_name} on {target_name} ({from}): {shots}{}{}",
                    if flags.is_empty() { String::new() } else { format!("; {}", flags.join(", ")) },
                    if w.weapon.is_some_and(|x| x != gun) { "; not the weapon named, which is no gun" } else { "" }
                );
            }
            let shooter = s.by.get_mut(&id).expect("inserted");
            shooter.wait = (shooter.wait - dt).max(0.0);
            let tchest = [tfeet[0], tfeet[1], tfeet[2] + (teyes[2] - tfeet[2]) * CHEST];
            // What holds its fire this frame.
            let restrained = g.extra.get(&me).is_some_and(|e| e.restrained);
            let away = w.target_here.and_then(|r| fnv_ai::goal::place_of_ref(g, r)).is_some_and(|p| (p.pos[0] - tfeet[0]).hypot(p.pos[1] - tfeet[1]) > TARGET_HERE_RADIUS);
            let blocker = if w.hold_fire_when_blocked {
                places
                    .iter()
                    .filter(|(k, _)| **k != id && **k != w.target.0)
                    .find(|(_, p)| tactics::in_line_of_fire(eyes, tchest, [p.0[0], p.0[1], p.0[2] + (p.1[2] - p.0[2]) * CHEST], tactics::LINE_OF_FIRE_WIDTH))
                    .map(|(k, _)| *k)
            } else {
                None
            };
            let why = if restrained {
                Some("restrained")
            } else if away {
                Some("its target is not where the use wants it")
            } else if blocker.is_some() {
                Some("someone stands in its line of fire")
            } else {
                None
            };
            if why != shooter.held {
                if let Some(why) = why {
                    match why {
                        "restrained" => s.stats.restrained += 1,
                        "someone stands in its line of fire" => s.stats.blocked += 1,
                        _ => s.stats.target_away += 1,
                    }
                    info!("use weapon: {name} {me} holds fire: {why}");
                }
                shooter.held = why;
            }
            let drawn = hold.is_none_or(|h| h.anim.stage == Stage::Drawn);
            let facing = facing_off(global, feet, tfeet) <= FIRE_FACING;
            let ready = why.is_none() && drawn && facing;
            let trigger = ready && shooter.wait <= 0.0 && (ws.automatic || !shooter.pulled);
            shooter.pulled = trigger;
            for e in shooter.cycle.step(&c.tuning, Input { trigger, reload: false, dt, carried: u32::MAX / 2 }) {
                match e {
                    Event::Fired { .. } => {
                        let muzzle = muzzle_at.get(&id).copied();
                        let to = vec::sub(tchest, eyes);
                        let aim = vec::scale(to, 1.0 / vec::len(to).max(1.0));
                        let origin = vec::add(eyes, vec::scale(aim, MUZZLE_AHEAD));
                        let cone = if w.always_hit {
                            0.0
                        } else {
                            let skill = ws.skill.as_deref().and_then(fnv_game::functions::av_by_name).map_or(50.0, |code| fnv_game::functions::actor_value_capped(g, me, code)) as f32;
                            let strength = fnv_game::functions::actor_value_capped(g, me, fnv_game::actor_values::STRENGTH) as f32;
                            let stance = Stance { aiming: false, moving: false, running: false, sneaking: false, airborne: false, skill, strength, aiming_move_divisor: 1.0, left_arm_crippled: false, right_arm_crippled: false };
                            spread::cone_deg(spread::actor_spread(&c.rules, &c.tuning, ws, &stance), 0.0, ws, &[])
                        };
                        fire.write(ActorFire { shooter: me, weapon: gun, origin, aim, cone, muzzle, harmless: w.no_damage });
                        anims.write(PlayAttack { actor: id, weapon: Some(gun), melee: false, sights: false, kind: Default::default() });
                        s.stats.shots += 1;
                        s.stats.sure += usize::from(w.always_hit);
                        s.stats.harmless += usize::from(w.no_damage);
                        let roll = next_unit(&mut s.rng);
                        let Some(u) = g.weapon_uses.get_mut(&me) else { break };
                        match u.shot(roll) {
                            After::More => {
                                if !ws.automatic {
                                    let [lo, hi] = ws.semi_auto_fire_delay;
                                    shooter.wait = lo + (hi - lo).max(0.0) * next_unit(&mut s.rng);
                                }
                            }
                            After::Pause(p) => {
                                shooter.wait = p;
                                // A volley is over: the trigger is let go.
                                shooter.pulled = true;
                            }
                            After::Done => {
                                info!("use weapon: {name} {me} has fired its {} shots at {target_name}", u.fired);
                                g.end_weapon_use(me);
                                s.stats.done += 1;
                                break;
                            }
                        }
                    }
                    Event::ReloadStarted => {
                        s.stats.reloads += 1;
                        if let Some(u) = g.weapon_uses.get_mut(&me).filter(|u| u.crouch_to_reload) {
                            u.crouched = true;
                            s.stats.crouched += 1;
                        }
                    }
                    Event::Reloaded { .. } => {
                        if let Some(u) = g.weapon_uses.get_mut(&me) {
                            u.crouched = false;
                        }
                    }
                    _ => {}
                }
            }
        }
        // Those no longer using a weapon stand up.
        s.by.retain(|id, _| still.contains(id));
        for (a, u) in g.weapon_uses.iter_mut() {
            if !still.contains(&a.0) {
                u.crouched = false;
            }
        }
    });
}

fn report(mut exit: MessageReader<AppExit>, shooters: Res<Shooters>, stats: Option<Res<CombatStats>>) {
    if exit.read().next().is_none() {
        return;
    }
    let s = &shooters.stats;
    let harmless_hits = stats.map_or(0, |c| c.harmless_hits);
    info!(
        "use weapon summary: {} actors used a weapon ({}); {} uses ({} from scripts, {} from packages), {} over; {} shots ({} always hit, {} harmless; {harmless_hits} harmless hits), {} reloads ({} crouched); held fire: {} line of fire, {} target away, {} restrained",
        s.actors.len(),
        s.actors.values().cloned().collect::<Vec<_>>().join(", "),
        s.from_scripts + s.from_packages,
        s.from_scripts,
        s.from_packages,
        s.done,
        s.shots,
        s.sure,
        s.harmless,
        s.reloads,
        s.crouched,
        s.blocked,
        s.target_away,
        s.restrained,
    );
}
