//! What a drawn NPC wears and holds is what the rules have it wear and hold
//! (`fnv_sim::World::equipment`: its equipped items once its cell was loaded, before that
//! the ones its outfitting will equip). Owner's playtest list 4 item 15: taking a dead
//! Powder Ganger's armour and weapon left them drawn on the body; and scripted
//! EquipItem/UnequipItem on NPCs changed only the rules' flags
//! (docs/design/script-function-audit.md §5).
//!
//! An NPC is spawned from `fnv_actor::resolve`'s own choice ([`Dressed`], put on by
//! `actors::spawn`) and redressed at once when the rules say otherwise, and again whenever
//! their equipped set changes (a script's EquipItem or UnequipItem, an item taken from a
//! corpse or removed): armour no longer worn is taken off and what it covered (the race's
//! body, hands or head, the hair's no-hat shape) is put on, armour now worn is put on, on
//! the same skeleton, so a ragdoll keeps its pose; a weapon no longer held is taken off
//! (its model in the hand or the holster, its muzzle, its clips), as a weapon that broke
//! (`weapons::drop_broken`), and one now held is put in the holster of a living actor
//! [design]. While the simulation holds no inventory for the actor (its cell never
//! loaded in the rules) the engine's own choice stays; it agrees with the rules' only
//! where no leveled choice is random (`fnvtool actor appearance` "random": false)
//! [observed]. Creatures' bodies are drawn as their records say and are not followed;
//! the weapon a creature holds is ([`arm_creatures`]).
//!
//! **Creatures' weapons** (owner's playtest list 7 item 39: a nightkin fought with its
//! fists, not its rebar club). Super mutants and nightkin are creatures (CREA, type Super
//! Mutant, on `creatures\smspinebreaker\skeleton.nif`) that carry weapons
//! [observed: FalloutNV.esm, e.g. Davison 0x0007F8E1 carries CondNVBumperSwordLoot]; their
//! skeleton folder has the people's weapon stances `2ha`, `2hh`, `2hl`, `2hm`, `2hr`,
//! `1gt` and `h2h` (equip, aim, attack, reload clips) but no `1hm` or `1hp`
//! [observed: `Fallout - Meshes.bsa` listing, 2026-10-23]. A creature now draws the weapon
//! the rules have it hold (`fnv_sim::World::equipment`: the drawable weapon with the
//! highest damage among what it carries, a hand weapon before a thrown one, as for NPCs;
//! docs/spec-notes/actors.md §9.3, §10.6 for every creature) whenever its skeleton folder has
//! that weapon's stance, as an NPC's (`actors::arm_weapon`: holstered, drawn when
//! alerted, its attack and reload clips from its own folder, `attack_anims.rs`), and
//! fights with it (`npc_combat`: a creature with a weapon drawn swings or fires it, as
//! an NPC) [design: which creatures draw is decided by the clips, not by the CREA "Weapon
//! & Shield" flag, hypothesis]. A creature whose folder has no stance for its weapon keeps
//! its own attack, as before. Its unarmed alert look (`creature_stance.rs`) gives way
//! to the weapon's draw cycle.

use bevy::prelude::*;
use fnv_actor::{Appearance, Part};
use fnv_sim::FormId;
use nvf_assets::actor::{spawn_parts, ActorCtx, ActorParts, SpawnedActor};
use nvf_assets::anim_play::BlendPlayer;
use nvf_assets::model::ModelCtx;
use nvf_play::sim::SimState;
use nvf_play::RefVisual;

use crate::actors::{part_uses, ActorStats, ActorWorld};
use crate::limbs::ActorRig;
use crate::ragdoll::Corpse;
use crate::weapons::{HeldWeapon, Muzzle, WeaponHold};

pub struct OutfitsPlugin;

impl Plugin for OutfitsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OutfitStats>().add_systems(Update, (follow_inventory, arm_creatures.after(crate::creature_stance::StanceAttach))).add_systems(Last, report);
    }
}

/// What an NPC is drawn wearing and holding, and the entities of each drawn part.
#[derive(Component, Debug, Clone)]
pub struct Dressed {
    pub worn: Vec<FormId>,
    pub weapon: Option<FormId>,
    /// The held weapon's mod flags (`Appearance::weapon_mods`).
    pub weapon_mods: u8,
    /// Each drawn part's key ([`part_key`]) and its shape entities.
    pub parts: Vec<(String, Vec<Entity>)>,
}

impl Dressed {
    /// The record of an actor just spawned from `a`: `drawn` and `part_of` as
    /// `spawn_actor` gave them, `part_index` the index in `a.parts` of each part spawned.
    pub fn new(world: &ActorWorld, a: &Appearance, part_index: &[usize], spawned: &SpawnedActor) -> Dressed {
        Dressed { worn: a.worn.clone(), weapon: a.weapon, weapon_mods: a.weapon_mods, parts: grouped(a, part_index, spawned, pipboy_of(world, a)) }
    }
}

/// The drawn shapes of `spawned` grouped by part (`part_index`: the index in `a.parts`
/// of each part given to the spawn).
fn grouped(a: &Appearance, part_index: &[usize], spawned: &SpawnedActor, pipboy: bool) -> Vec<(String, Vec<Entity>)> {
    let mut out: Vec<(String, Vec<Entity>)> = Vec::new();
    for (k, e) in spawned.drawn.iter().enumerate() {
        let key = part_key(a, &a.parts[part_index[spawned.part_of[k]]], pipboy);
        match out.iter_mut().find(|(n, _)| *n == key) {
            Some((_, v)) => v.push(*e),
            None => out.push((key, vec![*e])),
        }
    }
    out
}

/// A part's identity for telling which parts change when the outfit does: what it is,
/// where from, with what texture, and what decides the shapes it shows (the hair's hat
/// shape; an outfit's Pip-Boy sleeve).
pub fn part_key(a: &Appearance, p: &Part, pipboy: bool) -> String {
    let shows = match p.role.as_str() {
        "hair" => a.hat,
        "armor" => pipboy,
        _ => false,
    };
    format!("{}|{}|{}|{}|{shows}", p.role, p.form, p.model, p.texture.as_deref().unwrap_or(""))
}

/// Counts for the summary line.
#[derive(Resource, Default)]
pub struct OutfitStats {
    pub armour_off: usize,
    pub armour_on: usize,
    pub weapons_off: usize,
    pub weapons_on: usize,
    pub parts_on: usize,
    /// Creatures that took up a weapon ([`arm_creatures`]), and those whose skeleton has
    /// no stance for the weapon they hold.
    pub creatures_armed: usize,
    pub creatures_no_stance: usize,
}

/// On a drawn creature [`arm_creatures`] has looked at: the weapon it was given (or
/// could not be given) last.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CreatureArms {
    pub weapon: Option<FormId>,
}

/// A drawn NPC whose outfit follows the rules: its root, rig, outfit, clips (none in the
/// bind pose) and whether it is a corpse.
type Followed<'w> = (Entity, &'w ActorRig, &'w mut Dressed, Option<&'w mut BlendPlayer>, Has<Corpse>);

#[allow(clippy::too_many_arguments)]
fn follow_inventory(
    mut commands: Commands,
    world: Option<Res<ActorWorld>>,
    sim: Option<Res<SimState>>,
    mut parts: ResMut<ActorParts>,
    mut models: ModelCtx,
    mut stats: ResMut<OutfitStats>,
    mut actors: Query<Followed>,
    held: Query<(Entity, &HeldWeapon)>,
    muzzles: Query<(Entity, &Muzzle)>,
) {
    let (Some(world), Some(sim)) = (world, sim) else { return };
    for (entity, rig, mut dressed, mut player, dead) in &mut actors {
        let Some(items) = sim.world.equipment(FormId(rig.reference)) else { continue };
        let (worn, weapon) = drawn_of(&world, &items);
        // The mods of the unit held: a modded weapon draws its mods' model.
        let mods = weapon.map_or(0, |w| sim.world.peek(FormId(rig.reference)).and_then(|s| s.inventory.as_ref()).map_or(0, |inv| inv.mods_in_use(w)));
        let dressed = &mut *dressed;
        let same_worn = worn.len() == dressed.worn.len() && worn.iter().all(|w| dressed.worn.contains(w));
        let same_weapon = weapon == dressed.weapon && mods == dressed.weapon_mods;
        if same_worn && same_weapon {
            continue;
        }
        let who = format!("{:08X}", rig.reference);
        let mut new = match world.resolve_wearing(rig.base, rig.reference, Some(&worn)) {
            Ok(n) => n,
            Err(e) => {
                warn!("outfit: actor {who}: cannot be dressed again ({e})");
                (dressed.worn, dressed.weapon, dressed.weapon_mods) = (worn, weapon, mods);
                continue;
            }
        };
        new.weapon = weapon;
        new.weapon_mods = mods;
        let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
        let mut spawned = SpawnedActor { root: rig.nodes[0], nodes: rig.nodes.clone(), drawn: Vec::new(), part_of: Vec::new(), shape_of: Vec::new(), unbound: 0 };
        if !same_weapon {
            // The old hold goes, the weapon's or the fists' (`actors::arm`).
            for (e, _) in held.iter().filter(|(_, h)| h.reference == rig.reference) {
                commands.entity(e).despawn();
            }
            for (e, _) in muzzles.iter().filter(|(_, m)| m.reference == rig.reference) {
                commands.entity(e).despawn();
            }
            commands.entity(entity).remove::<WeaponHold>();
            if let Some(p) = player.as_deref_mut() {
                (p.layers[2], p.layers[3]) = (None, None);
            }
            if let Some(w) = dressed.weapon {
                stats.weapons_off += 1;
                info!("outfit: actor {who}: weapon {w} no longer held: taken off the body");
            }
            // A corpse is not armed; an actor drawn without clips has no holster pose.
            // Without a weapon it holds its fists.
            if let (Some(p), false) = (player.as_deref_mut(), dead) {
                let mut st = ActorStats::default();
                if let Some(hold) = crate::actors::arm(&mut commands, &spawned, &rig.skel, &new, rig.reference, &world, &mut ctx, p, &mut st) {
                    commands.entity(entity).insert(hold);
                    match weapon {
                        Some(w) if st.fists == 0 => {
                            stats.weapons_on += 1;
                            let modded = if mods != 0 { format!(" (mods {mods})") } else { String::new() };
                            info!("outfit: actor {who}: weapon {w}{modded} now held: put in its holster");
                        }
                        _ => info!("outfit: actor {who}: no weapon held: its fists"),
                    }
                }
            }
            (dressed.weapon, dressed.weapon_mods) = (weapon, mods);
        }
        if same_worn {
            continue;
        }
        let pipboy = pipboy_of(&world, &new);
        let new_keys: Vec<String> = new.parts.iter().map(|p| part_key(&new, p, pipboy)).collect();
        // Parts no longer drawn come off.
        let mut kept = Vec::new();
        for (key, shapes) in std::mem::take(&mut dressed.parts) {
            if new_keys.contains(&key) {
                kept.push((key, shapes));
            } else {
                for e in shapes {
                    commands.entity(e).despawn();
                }
            }
        }
        // Parts drawn now that were not: on the same skeleton.
        let have: Vec<String> = kept.iter().map(|(k, _)| k.clone()).collect();
        let mut put_on = new.clone();
        put_on.parts = new.parts.iter().zip(&new_keys).filter(|(_, k)| !have.contains(k)).map(|(p, _)| p.clone()).collect();
        let mut st = ActorStats::default();
        let (part_index, uses): (Vec<usize>, Vec<_>) = part_uses(&put_on, &world, &mut ctx, &mut st).into_iter().unzip();
        spawn_parts(&mut commands, &rig.skel, &uses, &mut spawned);
        for e in &spawned.drawn {
            commands.entity(*e).insert(RefVisual(rig.reference));
        }
        let added = grouped(&put_on, &part_index, &spawned, pipboy);
        stats.parts_on += added.len();
        kept.extend(added);
        dressed.parts = kept;
        let off: Vec<FormId> = dressed.worn.iter().copied().filter(|a| !worn.contains(a)).collect();
        let on: Vec<FormId> = worn.iter().copied().filter(|a| !dressed.worn.contains(a)).collect();
        stats.armour_off += off.len();
        stats.armour_on += on.len();
        let list = |v: &[FormId]| if v.is_empty() { "nothing".to_string() } else { v.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(", ") };
        info!(
            "outfit: actor {who}: armour {} taken off, {} put on; {} part(s) put on ({})",
            list(&off),
            list(&on),
            put_on.parts.len(),
            put_on.parts.iter().map(|p| p.role.as_str()).collect::<Vec<_>>().join(", ")
        );
        dressed.worn = worn;
    }
}

/// A drawn creature: its root, rig, clips, whether it is a corpse, and what it was armed with.
type Armed<'w> = (Entity, &'w ActorRig, Option<&'w mut BlendPlayer>, Has<Corpse>, Option<&'w CreatureArms>);

/// A creature takes up the weapon the rules have it hold, where its skeleton has that
/// weapon's stance (module notes).
#[allow(clippy::too_many_arguments)]
fn arm_creatures(
    mut commands: Commands,
    world: Option<Res<ActorWorld>>,
    sim: Option<Res<SimState>>,
    mut parts: ResMut<ActorParts>,
    mut models: ModelCtx,
    mut stats: ResMut<OutfitStats>,
    mut actors: Query<Armed, (Without<Dressed>, Without<crate::locomotion::Dying>)>,
    held: Query<(Entity, &HeldWeapon)>,
    muzzles: Query<(Entity, &Muzzle)>,
) {
    let (Some(world), Some(sim)) = (world, sim) else { return };
    if world.weapon_mode == crate::weapons::WeaponMode::None {
        return;
    }
    for (entity, rig, mut player, dead, arms) in &mut actors {
        if dead || rig.reference == FormId::PLAYER_REF.0 || !rig.skeleton.to_ascii_lowercase().contains("creatures") {
            continue;
        }
        let Some(p) = player.as_deref_mut() else { continue };
        let Some(items) = sim.world.equipment(FormId(rig.reference)) else { continue };
        let weapon = items.iter().copied().find(|i| world.actors.weapons.get(i).is_some_and(fnv_actor::weapon::drawable));
        let before = arms.and_then(|a| a.weapon);
        if arms.is_some() && weapon == before {
            continue;
        }
        commands.entity(entity).insert(CreatureArms { weapon });
        if arms.is_none() && weapon.is_none() {
            continue;
        }
        // The old weapon goes (model, muzzle, clips).
        if before.is_some() {
            for (e, _) in held.iter().filter(|(_, h)| h.reference == rig.reference) {
                commands.entity(e).despawn();
            }
            for (e, _) in muzzles.iter().filter(|(_, m)| m.reference == rig.reference) {
                commands.entity(e).despawn();
            }
            commands.entity(entity).remove::<WeaponHold>();
            (p.layers[2], p.layers[3]) = (None, None);
            info!("creature weapons: {:08X}: weapon {} no longer held", rig.reference, before.map_or(String::new(), |w| w.to_string()));
        }
        let Some(w) = weapon else { continue };
        let mods = sim.world.peek(FormId(rig.reference)).and_then(|s| s.inventory.as_ref()).map_or(0, |inv| inv.mods_in_use(w));
        let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
        let spawned = SpawnedActor { root: rig.nodes[0], nodes: rig.nodes.clone(), drawn: Vec::new(), part_of: Vec::new(), shape_of: Vec::new(), unbound: 0 };
        let mut st = ActorStats::default();
        let name = world.actors.weapons.get(&w).and_then(|x| x.edid.clone()).unwrap_or_default();
        let kind = world.actors.weapons.get(&w).and_then(|x| x.anim_type.clone()).unwrap_or_default();
        match crate::actors::arm_weapon(&mut commands, &spawned, &rig.skel, &rig.skeleton, Some(w), mods, rig.reference, &world, &mut ctx, p, &mut st) {
            Some(hold) => {
                // Its unarmed alert look gives way to the weapon's draw cycle.
                commands.entity(entity).remove::<crate::creature_stance::CreatureStance>().insert(hold);
                stats.creatures_armed += 1;
                info!("creature weapons: {:08X} ({}) holds {name} {w} ({kind}): put in its holster", rig.reference, rig.skeleton);
            }
            None => {
                stats.creatures_no_stance += 1;
                info!("creature weapons: {:08X} ({}) carries {name} {w} ({kind}) but its skeleton has no stance for it: it fights with its own attack", rig.reference, rig.skeleton);
            }
        }
    }
}

/// Of the rules' equipped `items`, the armour drawn (each piece once) and the weapon held
/// (the first that has a weapon record).
fn drawn_of(world: &ActorWorld, items: &[FormId]) -> (Vec<FormId>, Option<FormId>) {
    let mut worn: Vec<FormId> = Vec::new();
    for &i in items {
        if world.actors.armors.contains_key(&i) && !worn.contains(&i) {
            worn.push(i);
        }
    }
    (worn, items.iter().copied().find(|i| world.actors.weapons.contains_key(i)))
}

/// Whether appearance `a` wears a Pip-Boy (the outfits' sleeve shapes depend on it).
fn pipboy_of(world: &ActorWorld, a: &Appearance) -> bool {
    a.parts.iter().any(|p| world.actors.armors.get(&p.form).is_some_and(|ar| ar.slots & fnv_actor::resolve::slot::PIPBOY != 0))
}

fn report(mut exit: MessageReader<AppExit>, stats: Res<OutfitStats>) {
    if exit.read().next().is_some() {
        info!(
            "outfit summary: {} armour pieces taken off and {} put on, {} weapons taken off and {} put on drawn actors as the rules' equipped items changed; {} parts put on; {} creatures took up a weapon, {} hold one their skeleton has no stance for",
            stats.armour_off, stats.armour_on, stats.weapons_off, stats.weapons_on, stats.parts_on, stats.creatures_armed, stats.creatures_no_stance
        );
    }
}
