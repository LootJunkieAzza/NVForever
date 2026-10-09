//! The player's attacks with ranged weapons (docs/design/combat.md step C4), on the
//! headless rules of `fnv-combat` (docs/spec-notes/combat-rules.md):
//!
//! - **Weapon and ammo.** The equipped weapon (simulation) and its loaded ammo: the
//!   first of the weapon's ammo list the player carries (`CombatForms::ammo_for`). The
//!   magazine is part of the carried count, as rounds in the inventory; a shot removes
//!   `ammo_use` rounds from the inventory. A weapon with no ammo of its own but a
//!   projectile (grenades, thrown weapons) spends itself. A newly equipped weapon starts
//!   loaded with what is carried [design].
//! - **The cycle** (`fnv_combat::attack::Cycle`): Use is the trigger, a Ready Item tap
//!   reloads, Ammo Swap moves down the ammo list and reloads [design]. It runs only once
//!   the player body says the weapon is out ([`WeaponAnim::ready`]); the body plays the
//!   attack and reload clips when [`WeaponAnim::attacks`] / [`WeaponAnim::reloads`] count
//!   up and gives their timing back.
//! - **Shots** leave the eye along the view, inside the spread cone (the documented New
//!   Vegas spread plus bloom); recoil kicks the view up and recovers [design].
//! - **Projectiles** fly through the physics world (`fnv_phys` rays; actor hit volumes,
//!   combat C3). Hits are logged with the damage the rules give; applying damage to
//!   health is step C5. Melee and unarmed attacks are step C6.
//! - **Feedback** [design, until impact effects and audio exist]: a marker at each impact,
//!   tracers at the projectile's tracer chance, and an ammo counter.

use bevy::prelude::*;
use fnv_combat::attack::{ClipTiming, Cycle, Event, Input, Timing};
use fnv_combat::damage::{self, Attacker, AttackKind, HitInput, HitPart, NoPerks, Target};
use fnv_combat::data::AmmoEffect;
use fnv_combat::projectile::{blast, ActorId, CombatWorld, ProjEvent, Projectile, WorldHit};
use fnv_combat::rng::SplitMix;
use fnv_combat::spread::{self, Kick, Stance};
use fnv_combat::vec::{self, V3};
use fnv_combat::{shot, CombatForms, CombatTuning, Rules};
use fnv_phys::{layers, Filter, Key, Owner, Queries};
use fnv_sim::FormId;

use crate::control::PlayerControl;
use crate::phys::PhysState;
use crate::player::{Player, MAX_PITCH};
use crate::sim::SimState;
use crate::{space, PlaySet, PlayState, PlayWorld};

/// The shooter's id for projectiles (the player has no hit volume).
const PLAYER: ActorId = u64::MAX;

/// What the player body and the combat systems tell each other.
#[derive(Resource, Default, Debug, Clone)]
pub struct WeaponAnim {
    /// Set by the body: the weapon is drawn and in hand.
    pub ready: bool,
    /// Set by combat: the weapon's attack and reload animation names (`AttackRight`,
    /// `ReloadL`), for the body's clip choice.
    pub attack: Option<String>,
    pub reload: Option<String>,
    /// Set by the body: the attack and reload clips' timing in the rig in view.
    pub timing: ClipTiming,
    /// Counted up by combat when an attack or a reload starts; the body restarts the
    /// clip when it sees a new count.
    pub attacks: u32,
    pub reloads: u32,
}

/// Counts for the exit report.
#[derive(Resource, Default, Debug, Clone)]
pub struct CombatStats {
    pub shots: u32,
    pub projectiles: u32,
    pub actor_hits: u32,
    pub world_hits: u32,
    pub explosions: u32,
    pub damage: f32,
    pub reloads: u32,
    pub dry_fires: u32,
}

/// A projectile in flight with where it left from and whether it draws a tracer.
struct Flying {
    p: Projectile,
    origin: V3,
    tracer: bool,
    projectiles_per_shot: u32,
}

/// A hit waiting for its delivery time (hitscan: distance ÷ speed).
struct Pending {
    due: f32,
    actor: Option<ActorId>,
    point: V3,
    projectiles: u32,
}

/// A short-lived marker [design].
struct Mark {
    from: V3,
    to: V3,
    age: f32,
    life: f32,
    impact: bool,
}

/// The player's combat state.
#[derive(Resource)]
pub struct Combat {
    pub forms: CombatForms,
    pub rules: Rules,
    pub tuning: CombatTuning,
    weapon: Option<FormId>,
    ammo: Option<FormId>,
    /// The weapon spends itself (no ammo, a projectile).
    self_ammo: bool,
    cycle: Option<Cycle>,
    kick: Kick,
    flying: Vec<Flying>,
    pending: Vec<Pending>,
    marks: Vec<Mark>,
    rng: SplitMix,
    time: f32,
}

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WeaponAnim>()
            .init_resource::<CombatStats>()
            .add_systems(Startup, (init_combat, spawn_hud))
            .add_systems(
                Update,
                (equip_sync, fire, fly).chain().in_set(PlaySet::Activation).run_if(in_state(PlayState::Playing)).run_if(resource_exists::<Combat>),
            )
            .add_systems(Update, (draw_marks, update_hud).run_if(resource_exists::<Combat>))
            .add_systems(Last, report);
    }
}

fn init_combat(mut commands: Commands, d: Res<PlayWorld>) {
    let t = std::time::Instant::now();
    let forms = CombatForms::build(d.lo, d.schema);
    let rules = Rules::from_forms(&forms);
    info!("combat: {} weapons, {} ammo, {} projectiles, {} explosions read in {:.0} ms", forms.weapons.len(), forms.ammo.len(), forms.projectiles.len(), forms.explosions.len(), t.elapsed().as_secs_f64() * 1000.0);
    commands.insert_resource(Combat {
        forms,
        rules,
        tuning: CombatTuning::default(),
        weapon: None,
        ammo: None,
        self_ammo: false,
        cycle: None,
        kick: Kick::default(),
        flying: Vec::new(),
        pending: Vec::new(),
        marks: Vec::new(),
        rng: SplitMix(1),
        time: 0.0,
    });
}

/// How many of `item` the player carries.
fn player_count(sim: &SimState, item: Option<FormId>) -> u32 {
    let inv = sim.world.peek(FormId::PLAYER_REF).and_then(|s| s.inventory.as_ref());
    item.and_then(|i| inv.map(|inv| inv.count(i))).unwrap_or(0).max(0) as u32
}

/// Follow the equipped weapon: a new weapon gets a new cycle, loaded from what is carried.
fn equip_sync(mut c: ResMut<Combat>, sim: Res<SimState>, mut anim: ResMut<WeaponAnim>) {
    let c = &mut *c;
    let equipped = sim.world.peek(FormId::PLAYER_REF).and_then(|s| s.inventory.as_ref()).map(|i| i.equipped()).unwrap_or_default();
    let weapon = equipped.into_iter().find(|id| c.forms.weapons.contains_key(id));
    if weapon == c.weapon {
        return;
    }
    c.weapon = weapon;
    c.cycle = None;
    c.ammo = None;
    let Some(id) = weapon else {
        anim.attack = None;
        anim.reload = None;
        return;
    };
    let w = &c.forms.weapons[&id];
    let ammo = c.forms.ammo_for(id, |a| player_count(&sim, Some(a)) > 0);
    let chain = c.forms.chain(id, ammo).expect("weapon present");
    let melee = AttackKind::of(w) != AttackKind::Gun;
    c.self_ammo = !melee && chain.ammo_choices.is_empty() && chain.projectile.is_some();
    c.ammo = chain.ammo;
    let carried = player_count(&sim, c.ammo);
    let timing = Timing::of(w, &anim.timing, &c.tuning);
    let no_ammo = melee || c.self_ammo;
    c.cycle = Some(Cycle::new(w, timing, no_ammo, carried));
    anim.attack = w.attack_anim.clone();
    anim.reload = w.reload_anim.clone();
    info!(
        "combat: weapon {} ({id}), ammo {}, magazine {}/{}, {} projectile(s) per shot",
        w.edid.as_deref().unwrap_or("?"),
        c.ammo.and_then(|a| c.forms.ammo.get(&a)).and_then(|a| a.edid.clone()).unwrap_or_else(|| if c.self_ammo { "itself".into() } else { "none".into() }),
        c.cycle.as_ref().map_or(0, |cy| cy.magazine),
        w.clip_size,
        chain.projectiles_per_shot
    );
}

/// The physics world as combat sees it: rays that skip the player's capsule and the
/// layers nothing solid is on, and actor hit volumes.
struct PhysView<'a> {
    q: Queries<'a>,
    player: Option<Key>,
    actors: Vec<(ActorId, V3, f32)>,
}

impl CombatWorld for PhysView<'_> {
    fn ray(&self, from: V3, to: V3, ignore: &[ActorId]) -> Option<WorldHit> {
        let d = vec::sub(to, from);
        let len = vec::len(d);
        if len < 1e-4 {
            return None;
        }
        // Shots pass what the character passes (triggers, zones, water, …) [design].
        let mut f = Filter { exclude_layers: layers::character_ignores(), ..Default::default() };
        f.exclude.extend(self.player);
        f.exclude.extend(ignore.iter().filter(|&&a| a != PLAYER).map(|&a| Key::Actor(a as u32)));
        let h = self.q.ray(from, d, len, &f)?;
        Some(WorldHit {
            point: h.point,
            normal: h.normal,
            distance: h.distance,
            actor: match h.owner {
                Owner::Actor { form_id, .. } => Some(form_id as ActorId),
                _ => None,
            },
            small_transparent: matches!(h.owner, Owner::Ref { layer, .. } if layer == layers::TRANSPARENT_SMALL || layer == layers::TRANSPARENT_SMALL_ANIM),
            material: h.material,
        })
    }

    fn actors_near(&self, point: V3, radius: f32) -> Vec<(ActorId, V3)> {
        self.actors.iter().filter(|(_, c, r)| vec::len(vec::sub(*c, point)) <= radius + r).map(|&(id, c, _)| (id, c)).collect()
    }
}

fn view(phys: &mut PhysState) -> PhysView<'_> {
    let actors: Vec<(ActorId, V3, f32)> =
        phys.world.actor_volumes().filter_map(|id| phys.world.actor_volume(id).map(|v| (id as ActorId, v.center(), v.radius))).collect();
    let player = phys.player.as_ref().map(|c| c.key());
    phys.world.update_queries();
    PhysView { q: phys.world.queries(), player, actors }
}

/// The player's attacker values: S.P.E.C.I.A.L. from the simulation, the weapon's skill
/// derived from its attribute and Luck (skills are not in the simulation yet).
fn attacker(sim: &mut SimState, skill: Option<&str>, aiming_vats: bool) -> Attacker {
    let mut av = |n: &str| sim.world.actor_value(FormId::PLAYER_REF, n).unwrap_or(5.0);
    let (strength, luck, endurance) = (av("Strength"), av("Luck"), av("Endurance"));
    let skill_v = skill.and_then(damage::governing_special).map_or(50.0, |a| damage::derived_skill(av(a), luck, false));
    Attacker { skill: skill_v, strength, luck, unarmed_skill: damage::derived_skill(endurance, luck, false), in_vats: aiming_vats, ..Attacker::default() }
}

#[allow(clippy::too_many_arguments)]
fn fire(
    time: Res<Time>,
    mut c: ResMut<Combat>,
    mut sim: ResMut<SimState>,
    phys: Res<PhysState>,
    mut player: ResMut<Player>,
    mut control: ResMut<PlayerControl>,
    mut anim: ResMut<WeaponAnim>,
    mut stats: ResMut<CombatStats>,
) {
    let dt = time.delta_secs();
    let c = &mut *c;
    c.time += dt;
    // Recoil recovers whatever happens.
    let back = c.kick.recover(&c.tuning, dt);
    player.pitch = (player.pitch - back.to_radians()).clamp(-MAX_PITCH, MAX_PITCH);
    let reload = std::mem::take(&mut control.reload);
    let swap = std::mem::take(&mut control.ammo_swap);
    let (Some(id), Some(cycle)) = (c.weapon, c.cycle.as_mut()) else { return };
    if !anim.ready {
        return;
    }
    let w = c.forms.weapons[&id].clone();
    cycle.timing = Timing::of(&w, &anim.timing, &c.tuning);
    // Ammo Swap: the next carried type down the list, then a reload [design].
    let mut reload = reload;
    if swap {
        if let Some(cur) = c.ammo {
            let next = c.forms.next_ammo(id, cur, |a| player_count(&sim, Some(a)) > 0);
            if next.is_some_and(|n| n != cur) {
                c.ammo = next;
                cycle.magazine = 0;
                reload = true;
                info!("combat: ammo swapped to {}", next.and_then(|a| c.forms.ammo.get(&a)).and_then(|a| a.edid.clone()).unwrap_or_default());
            }
        }
    }
    let total = if c.self_ammo { player_count(&sim, Some(id)) } else { player_count(&sim, c.ammo) };
    let carried = total.saturating_sub(cycle.magazine);
    let events = cycle.step(&c.tuning, Input { trigger: control.trigger, reload, dt, carried });
    let Some(chain) = c.forms.chain(id, c.ammo) else { return };
    for e in events {
        match e {
            Event::Fired { rounds } => {
                anim.attacks += 1;
                stats.shots += 1;
                if rounds > 0 {
                    if let Some(a) = c.ammo {
                        let _ = sim.world.remove_item(FormId::PLAYER_REF, a, rounds as i32);
                    }
                }
                if c.self_ammo {
                    let _ = sim.world.remove_item(FormId::PLAYER_REF, id, 1);
                }
                if chain.projectile.is_none() {
                    debug!("combat: {} attacks (melee and unarmed hits are step C6)", w.edid.as_deref().unwrap_or("?"));
                    continue;
                }
                let effects: Vec<&AmmoEffect> = chain.ammo_effects.iter().filter_map(|e| c.forms.ammo_effects.get(e)).collect();
                let stance = Stance {
                    aiming: control.aiming,
                    moving: phys.player.as_ref().is_some_and(|ch| ch.velocity[0].hypot(ch.velocity[1]) > 1.0),
                    running: control.always_run,
                    sneaking: control.sneak,
                    airborne: phys.player.as_ref().is_some_and(|ch| !ch.grounded && !ch.swimming),
                    skill: attacker(&mut sim, w.skill.as_deref(), false).skill,
                    strength: sim.world.actor_value(FormId::PLAYER_REF, "Strength").unwrap_or(5.0),
                    aiming_move_divisor: 1.0,
                    ..Stance::default()
                };
                let actor_spread = spread::actor_spread(&c.rules, &c.tuning, &w, &stance);
                let cone = spread::cone_deg(actor_spread, c.kick.bloom, &w, &effects);
                let eye = player.eye();
                let aim = player.look_dir();
                let gravity = phys.world.params.gravity;
                let rng = &mut c.rng;
                let mut rand = || rng.next_f32();
                let shots = shot::fire(&c.forms, &chain, eye, aim, cone, Some(PLAYER), gravity, &c.tuning, &mut rand);
                let tracer_chance = chain.projectile.and_then(|p| c.forms.projectiles.get(&p)).map_or(0.0, |p| p.tracer_chance);
                stats.projectiles += shots.len() as u32;
                for p in shots {
                    let tracer = c.rng.next_f32() < tracer_chance;
                    c.flying.push(Flying { p, origin: eye, tracer, projectiles_per_shot: chain.projectiles_per_shot });
                }
                let kick = c.kick.shot(&c.tuning, w.min_spread, w.base_damage as f32);
                player.pitch = (player.pitch + kick.to_radians()).clamp(-MAX_PITCH, MAX_PITCH);
                debug!("combat: shot, cone {cone:.2}°, magazine {}", cycle.magazine);
            }
            Event::ReloadStarted => {
                anim.reloads += 1;
                debug!("combat: reloading");
            }
            Event::Reloaded { taken } => {
                stats.reloads += 1;
                info!("combat: reloaded {taken}, magazine {}", cycle.magazine);
            }
            Event::DryFire => {
                stats.dry_fires += 1;
                info!("combat: click (no ammo)");
            }
            Event::Regenerated => {}
        }
    }
}

/// Fly the projectiles, deliver hits when due, set off explosions.
fn fly(time: Res<Time>, mut c: ResMut<Combat>, mut sim: ResMut<SimState>, mut phys: ResMut<PhysState>, play: Res<PlayWorld>, mut stats: ResMut<CombatStats>) {
    let dt = time.delta_secs();
    let c = &mut *c;
    if c.flying.is_empty() && c.pending.is_empty() {
        return;
    }
    let mut explode: Vec<(FormId, V3)> = Vec::new();
    {
        let world = view(&mut phys);
        for f in &mut c.flying {
            let before = f.p.pos;
            for e in f.p.step(&world, dt) {
                match e {
                    ProjEvent::Hit { actor, point, delay, .. } => {
                        c.pending.push(Pending { due: c.time + delay, actor, point, projectiles: f.projectiles_per_shot });
                        if f.tracer {
                            let from = if f.p.hitscan { f.origin } else { before };
                            c.marks.push(Mark { from, to: point, age: 0.0, life: 0.08, impact: false });
                        }
                    }
                    ProjEvent::Explode { explosion, point } => explode.push((explosion, point)),
                    ProjEvent::Expired { .. } => {}
                }
            }
            if f.tracer && f.p.alive {
                c.marks.push(Mark { from: before, to: f.p.pos, age: 0.0, life: 0.05, impact: false });
            }
        }
        c.flying.retain(|f| f.p.alive);
        for (id, at) in explode {
            stats.explosions += 1;
            let Some(x) = c.forms.explosions.get(&id) else { continue };
            for b in blast(&world, &c.tuning, x, at) {
                stats.damage += b.damage;
                info!("combat: blast {} hits {:08X} at {:.0} for {:.1} (before armour)", x.edid.as_deref().unwrap_or("?"), b.actor, b.distance, b.damage);
            }
            c.marks.push(Mark { from: at, to: at, age: 0.0, life: 0.6, impact: true });
            info!("combat: explosion {} at ({:.0}, {:.0}, {:.0})", x.edid.as_deref().unwrap_or("?"), at[0], at[1], at[2]);
        }
    }
    let now = c.time;
    let due: Vec<Pending> = {
        let (d, keep): (Vec<Pending>, Vec<Pending>) = std::mem::take(&mut c.pending).into_iter().partition(|p| p.due <= now);
        c.pending = keep;
        d
    };
    let Some(id) = c.weapon else { return };
    let w = c.forms.weapons[&id].clone();
    let chain = c.forms.chain(id, c.ammo);
    let effect_ids = chain.map(|ch| ch.ammo_effects).unwrap_or_default();
    for p in due {
        c.marks.push(Mark { from: p.point, to: p.point, age: 0.0, life: 0.3, impact: true });
        let Some(actor) = p.actor else {
            stats.world_hits += 1;
            continue;
        };
        stats.actor_hits += 1;
        let effects: Vec<&AmmoEffect> = effect_ids.iter().filter_map(|e| c.forms.ammo_effects.get(e)).collect();
        let a = attacker(&mut sim, w.skill.as_deref(), false);
        let h = HitInput { weapon: &w, projectiles: p.projectiles, condition: 1.0, ammo_effects: &effects, part: HitPart::default() };
        let roll = c.rng.next_f32();
        let d = damage::hit(&c.rules, &c.tuning, &h, &a, &Target::default(), &NoPerks, &NoPerks, roll);
        stats.damage += d.dealt;
        let name = play.index.refs.get(&(actor as u32)).map(|e| e.sig).map_or("?".to_string(), |s| String::from_utf8_lossy(&s).into_owned());
        info!("combat: hit {name} {:08X} for {:.1}{} (health damage is step C5)", actor, d.dealt, if d.critical { ", critical" } else { "" });
    }
}

fn draw_marks(time: Res<Time>, mut c: ResMut<Combat>, mut gizmos: Gizmos) {
    let dt = time.delta_secs();
    for m in &mut c.marks {
        m.age += dt;
        let fade = (1.0 - m.age / m.life).clamp(0.0, 1.0);
        if m.impact {
            let p = space::to_bevy(m.to);
            let s = 4.0;
            let col = Color::srgba(1.0, 0.8, 0.3, fade);
            gizmos.line(p - Vec3::X * s, p + Vec3::X * s, col);
            gizmos.line(p - Vec3::Y * s, p + Vec3::Y * s, col);
            gizmos.line(p - Vec3::Z * s, p + Vec3::Z * s, col);
        } else {
            gizmos.line(space::to_bevy(m.from), space::to_bevy(m.to), Color::srgba(1.0, 0.9, 0.6, fade));
        }
    }
    c.marks.retain(|m| m.age < m.life);
}

#[derive(Component)]
struct AmmoText;

fn spawn_hud(mut commands: Commands) {
    commands.spawn((
        Node { position_type: PositionType::Absolute, right: Val::Px(24.0), bottom: Val::Px(20.0), ..default() },
        Text::new(""),
        TextFont { font_size: FontSize::Px(20.0), ..default() },
        TextColor(Color::srgb(1.0, 0.85, 0.4)),
        AmmoText,
    ));
}

/// The ammo counter: rounds in the magazine / rounds carried besides, and the ammo's
/// abbreviation (the game's HUD layout is not copied; ours [design]).
fn update_hud(state: Res<State<PlayState>>, c: Res<Combat>, sim: Res<SimState>, control: Res<PlayerControl>, mut text: Query<&mut Text, With<AmmoText>>) {
    let Ok(mut t) = text.single_mut() else { return };
    let line = match (c.weapon, c.cycle.as_ref()) {
        (Some(_), Some(cy)) if *state.get() == PlayState::Playing && control.weapon_out && !cy.no_ammo => {
            let total = player_count(&sim, c.ammo);
            let abbrev = c.ammo.and_then(|a| c.forms.ammo.get(&a)).and_then(|a| a.abbrev.clone().or(a.short_name.clone())).unwrap_or_default();
            format!("{} / {}  {}", cy.magazine.min(total), total.saturating_sub(cy.magazine), abbrev)
        }
        (Some(id), Some(_)) if *state.get() == PlayState::Playing && control.weapon_out && c.self_ammo => format!("{}", player_count(&sim, Some(id))),
        _ => String::new(),
    };
    if t.0 != line {
        t.0 = line;
    }
}

fn report(mut exit: MessageReader<AppExit>, stats: Res<CombatStats>) {
    if exit.read().last().is_some() {
        let s = &*stats;
        info!(
            "combat: {} shots, {} projectiles, {} actor hits, {} world hits, {} explosions, {:.0} damage (not applied yet), {} reloads, {} dry fires",
            s.shots, s.projectiles, s.actor_hits, s.world_hits, s.explosions, s.damage, s.reloads, s.dry_fires
        );
    }
}

/// The cycle's clip timing from the attack clip (length, `Hit` key) and the reload
/// clip's length, as the player body reads them.
pub fn clip_timing(attack: Option<(f32, Option<f32>)>, reload: Option<f32>) -> ClipTiming {
    ClipTiming { attack_len: attack.map(|a| a.0).filter(|l| *l > 0.0), hit_time: attack.and_then(|a| a.1), reload_len: reload.filter(|l| *l > 0.0) }
}
