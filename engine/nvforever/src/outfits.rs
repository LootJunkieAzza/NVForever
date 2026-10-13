//! What a drawn NPC wears follows its inventory (owner's playtest list 4 item 15: taking
//! a dead Powder Ganger's armour and weapon left them drawn on the body).
//!
//! An NPC is drawn wearing the armour and holding the weapon `fnv_actor::resolve` chose
//! from its carried items ([`Dressed`], put on by `actors::spawn`). When one of them
//! leaves the actor's inventory in the simulation (taken from a corpse, or a script's
//! RemoveItem), it leaves the drawn body too: the armour's model is taken off and what it
//! covered (the race's body, hands or head, the hair's no-hat shape) is put on the same
//! skeleton, so a ragdoll keeps its pose; the weapon's model (in the hand or the
//! holster) and its muzzle are taken off and the weapon clips stop, as a weapon that
//! broke (`weapons::drop_broken`) [design].
//!
//! The drawn choice and the simulation's roll of leveled lists are made apart, so they
//! agree only where no leveled choice is random (`fnvtool actor appearance` "random":
//! false, e.g. GSPG01Ref; the tribal village's dead Fiends are random) [observed]. What
//! stands for a drawn item in the inventory, looked at once when the simulation first
//! holds the actor's inventory: the item itself if carried; else the carried armour
//! sharing a body slot with it (the simulation's roll of that outfit), or for the weapon
//! every carried weapon; nothing at all leaves it drawn. A drawn item comes off when
//! everything standing for it has left the inventory [design].

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
    /// For each of `worn` and `weapon`, the carried items standing for it (module
    /// notes); `None` until the simulation holds the actor's inventory.
    pub backing: Option<Vec<(FormId, Vec<FormId>)>>,
}

impl Dressed {
    /// The record of an actor just spawned from `a`: `drawn` and `part_of` as
    /// `spawn_actor` gave them, `part_index` the index in `a.parts` of each part spawned.
    pub fn new(world: &ActorWorld, a: &Appearance, part_index: &[usize], spawned: &SpawnedActor) -> Dressed {
        Dressed { worn: a.worn.clone(), weapon: a.weapon, parts: grouped(a, part_index, spawned, pipboy_of(world, a)), backing: None }
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
    pub weapons_off: usize,
    pub parts_on: usize,
}

#[allow(clippy::too_many_arguments)]
fn follow_inventory(
    mut commands: Commands,
    world: Option<Res<ActorWorld>>,
    sim: Option<Res<SimState>>,
    mut parts: ResMut<ActorParts>,
    mut models: ModelCtx,
    mut stats: ResMut<OutfitStats>,
    mut actors: Query<(Entity, &ActorRig, &mut Dressed, Option<&mut BlendPlayer>)>,
    held: Query<(Entity, &HeldWeapon)>,
    muzzles: Query<(Entity, &Muzzle)>,
) {
    let (Some(world), Some(sim)) = (world, sim) else { return };
    for (entity, rig, mut dressed, player) in &mut actors {
        // An inventory the simulation has not made yet is the one rolled at the start.
        let Some(inv) = sim.world.peek(FormId(rig.reference)).and_then(|s| s.inventory.as_ref()) else { continue };
        let dressed = &mut *dressed;
        let backing = dressed.backing.get_or_insert_with(|| {
            let slots = |id: &FormId| world.actors.armors.get(id).map_or(0, |a| a.slots);
            let mut out: Vec<(FormId, Vec<FormId>)> = dressed
                .worn
                .iter()
                .map(|w| {
                    let by = if inv.count(*w) > 0 { vec![*w] } else { inv.iter().filter(|(i, n)| *n > 0 && slots(i) & slots(w) != 0).map(|(i, _)| i).collect() };
                    (*w, by)
                })
                .collect();
            if let Some(w) = dressed.weapon {
                let by = if inv.count(w) > 0 { vec![w] } else { inv.iter().filter(|(i, n)| *n > 0 && world.actors.weapons.contains_key(i)).map(|(i, _)| i).collect() };
                out.push((w, by));
            }
            out
        });
        let gone: Vec<FormId> = backing.iter().filter(|(_, by)| !by.is_empty() && by.iter().all(|i| inv.count(*i) <= 0)).map(|(item, _)| *item).collect();
        if gone.is_empty() {
            continue;
        }
        backing.retain(|(item, _)| !gone.contains(item));
        let who = format!("{:08X}", rig.reference);
        if let Some(w) = dressed.weapon.filter(|w| gone.contains(w)) {
            dressed.weapon = None;
            for (e, _) in held.iter().filter(|(_, h)| h.reference == rig.reference) {
                commands.entity(e).despawn();
            }
            for (e, _) in muzzles.iter().filter(|(_, m)| m.reference == rig.reference) {
                commands.entity(e).despawn();
            }
            commands.entity(entity).remove::<WeaponHold>();
            if let Some(mut p) = player {
                (p.layers[2], p.layers[3]) = (None, None);
            }
            stats.weapons_off += 1;
            info!("outfit: actor {who}'s weapon {w} left its inventory: taken off the body");
        }
        let off: Vec<FormId> = dressed.worn.iter().copied().filter(|a| gone.contains(a)).collect();
        if off.is_empty() {
            continue;
        }
        let keep: Vec<FormId> = dressed.worn.iter().copied().filter(|a| !off.contains(a)).collect();
        let new = match world.resolve_wearing(rig.base, rig.reference, Some(&keep)) {
            Ok(n) => n,
            Err(e) => {
                warn!("outfit: actor {who}: cannot be dressed again ({e})");
                dressed.worn = keep;
                continue;
            }
        };
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
        let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
        let mut st = ActorStats::default();
        let (part_index, uses): (Vec<usize>, Vec<_>) = part_uses(&put_on, &world, &mut ctx, &mut st).into_iter().unzip();
        let mut spawned = SpawnedActor { root: rig.nodes[0], nodes: rig.nodes.clone(), drawn: Vec::new(), part_of: Vec::new(), shape_of: Vec::new(), unbound: 0 };
        spawn_parts(&mut commands, &rig.skel, &uses, &mut spawned);
        for e in &spawned.drawn {
            commands.entity(*e).insert(RefVisual(rig.reference));
        }
        let added = grouped(&put_on, &part_index, &spawned, pipboy);
        stats.parts_on += added.len();
        kept.extend(added);
        dressed.parts = kept;
        stats.armour_off += off.len();
        info!(
            "outfit: actor {who}: {} left its inventory: taken off the body; {} part(s) put on in its place ({})",
            off.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(", "),
            put_on.parts.len(),
            put_on.parts.iter().map(|p| p.role.as_str()).collect::<Vec<_>>().join(", ")
        );
        dressed.worn = keep;
    }
}

/// Whether appearance `a` wears a Pip-Boy (the outfits' sleeve shapes depend on it).
fn pipboy_of(world: &ActorWorld, a: &Appearance) -> bool {
    a.parts.iter().any(|p| world.actors.armors.get(&p.form).is_some_and(|ar| ar.slots & fnv_actor::resolve::slot::PIPBOY != 0))
}

fn report(mut exit: MessageReader<AppExit>, stats: Res<OutfitStats>) {
    if exit.read().next().is_some() {
        info!("outfit summary: {} armour pieces and {} weapons taken off drawn actors as they left their inventories; {} parts put on in their place", stats.armour_off, stats.weapons_off, stats.parts_on);
    }
}
