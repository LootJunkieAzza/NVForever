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
//! [observed]. Creatures are drawn as their records say and are not followed.

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
        app.init_resource::<OutfitStats>().add_systems(Update, follow_inventory).add_systems(Last, report);
    }
}

/// What an NPC is drawn wearing and holding, and the entities of each drawn part.
#[derive(Component, Debug, Clone)]
pub struct Dressed {
    pub worn: Vec<FormId>,
    pub weapon: Option<FormId>,
    /// Each drawn part's key ([`part_key`]) and its shape entities.
    pub parts: Vec<(String, Vec<Entity>)>,
}

impl Dressed {
    /// The record of an actor just spawned from `a`: `drawn` and `part_of` as
    /// `spawn_actor` gave them, `part_index` the index in `a.parts` of each part spawned.
    pub fn new(world: &ActorWorld, a: &Appearance, part_index: &[usize], spawned: &SpawnedActor) -> Dressed {
        Dressed { worn: a.worn.clone(), weapon: a.weapon, parts: grouped(a, part_index, spawned, pipboy_of(world, a)) }
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
        let dressed = &mut *dressed;
        let same_worn = worn.len() == dressed.worn.len() && worn.iter().all(|w| dressed.worn.contains(w));
        if same_worn && weapon == dressed.weapon {
            continue;
        }
        let who = format!("{:08X}", rig.reference);
        let mut new = match world.resolve_wearing(rig.base, rig.reference, Some(&worn)) {
            Ok(n) => n,
            Err(e) => {
                warn!("outfit: actor {who}: cannot be dressed again ({e})");
                (dressed.worn, dressed.weapon) = (worn, weapon);
                continue;
            }
        };
        new.weapon = weapon;
        let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
        let mut spawned = SpawnedActor { root: rig.nodes[0], nodes: rig.nodes.clone(), drawn: Vec::new(), part_of: Vec::new(), shape_of: Vec::new(), unbound: 0 };
        if weapon != dressed.weapon {
            if let Some(w) = dressed.weapon {
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
                stats.weapons_off += 1;
                info!("outfit: actor {who}: weapon {w} no longer held: taken off the body");
            }
            // A corpse is not armed; an actor drawn without clips has no holster pose.
            if let (Some(w), Some(p), false) = (weapon, player.as_deref_mut(), dead) {
                let mut st = ActorStats::default();
                if let Some(hold) = crate::actors::arm(&mut commands, &spawned, &rig.skel, &new, rig.reference, &world, &mut ctx, p, &mut st) {
                    commands.entity(entity).insert(hold);
                    stats.weapons_on += 1;
                    info!("outfit: actor {who}: weapon {w} now held: put in its holster");
                }
            }
            dressed.weapon = weapon;
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
            "outfit summary: {} armour pieces taken off and {} put on, {} weapons taken off and {} put on drawn actors as the rules' equipped items changed; {} parts put on",
            stats.armour_off, stats.armour_on, stats.weapons_off, stats.weapons_on, stats.parts_on
        );
    }
}
