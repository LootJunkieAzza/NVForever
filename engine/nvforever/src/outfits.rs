//! What a drawn NPC wears and holds is what the rules have it wear and hold
//! (`fnv_sim::World::equipment`: its equipped items once its cell was loaded, before that
//! the ones its outfitting will equip). Owner's playtest list 4 item 15: taking a dead
//! Powder Ganger's armour and weapon left them drawn on the body; and scripted
//! EquipItem/UnequipItem on NPCs changed only the rules' flags
//! (docs/design/script-function-audit.md §5).
//!
//! An NPC is spawned from its simulation equipment when available, otherwise from
//! `fnv_actor::resolve`'s own choice ([`Dressed`], put on by `actors::spawn`)
//! [observed: `appearance`]. It is redressed whenever
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

/// The simulation's drawable equipment, shared by spawning, prefetch and reconciliation.
/// `None` means no simulated inventory yet; an empty or partial set is authoritative
/// [observed: `fnv_sim::World::equipment`], including after looting or UnequipItem.
#[derive(Debug, PartialEq, Eq)]
pub struct Equipment {
    pub worn: Vec<FormId>,
    pub weapon: Option<FormId>,
    pub weapon_mods: u8,
}

impl Equipment {
    pub fn of(world: &ActorWorld, sim: Option<&fnv_sim::World<'_>>, reference: u32) -> Option<Self> {
        let sim = sim?;
        let id = FormId(reference);
        let items = sim.equipment(id)?;
        Self::select(&world.actors, Some(&items), |w| sim.peek(id).and_then(|s| s.inventory.as_ref()).map_or(0, |inv| inv.mods_in_use(w)))
    }

    fn select(actors: &fnv_actor::ActorForms, items: Option<&[FormId]>, mods: impl FnOnce(FormId) -> u8) -> Option<Self> {
        let items = items?;
        let mut worn = Vec::new();
        for &i in items {
            if actors.armors.contains_key(&i) && !worn.contains(&i) {
                worn.push(i);
            }
        }
        let weapon = items.iter().copied().find(|i| actors.weapons.contains_key(i));
        Some(Self { worn, weapon, weapon_mods: weapon.map_or(0, mods) })
    }
}

/// Resolve the same appearance for spawn, read-ahead and later outfit changes. Passing
/// the held weapon to the resolver also applies its Hide Backpack flag [observed:
/// `Resolver::resolve_changed_at`]; its record-selected weapon is then replaced.
pub fn appearance(world: &ActorWorld, base: u32, reference: u32, equipment: Option<&Equipment>) -> Result<Appearance, String> {
    let Some(e) = equipment else { return world.resolve(base, reference) };
    let wearing: Vec<_> = e.worn.iter().copied().chain(e.weapon).collect();
    let mut a = world.resolve_wearing(base, reference, Some(&wearing))?;
    a.weapon = e.weapon;
    a.weapon_mods = e.weapon_mods;
    Ok(a)
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
    /// Actors whose reconciled outfit was committed (one for armour and weapon together).
    pub replacements: usize,
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
    let profiling = crate::profile::interval().is_some();
    let (mut replacements, mut replacement_ms) = (0, 0.0);
    for (entity, rig, mut dressed, mut player, dead) in &mut actors {
        let Some(equipment) = Equipment::of(&world, Some(&sim.world), rig.reference) else { continue };
        let Equipment { ref worn, weapon, weapon_mods: mods } = equipment;
        let dressed = &mut *dressed;
        // A weapon with "Hide Backpack" taken up or put away dresses the body again
        // (`Resolver::resolve_wearing`).
        let hides = |w: Option<FormId>| w.and_then(|w| world.actors.weapons.get(&w)).is_some_and(|w| w.hide_backpack);
        let same_worn = worn.len() == dressed.worn.len() && worn.iter().all(|w| dressed.worn.contains(w)) && hides(weapon) == hides(dressed.weapon);
        let same_weapon = weapon == dressed.weapon && mods == dressed.weapon_mods;
        if same_worn && same_weapon {
            continue;
        }
        let who = format!("{:08X}", rig.reference);
        let started = profiling.then(std::time::Instant::now);
        let new = match appearance(&world, rig.base, rig.reference, Some(&equipment)) {
            Ok(n) => n,
            Err(e) => {
                warn!("outfit: actor {who}: cannot be dressed again ({e})");
                (dressed.worn, dressed.weapon, dressed.weapon_mods) = (worn.clone(), weapon, mods);
                continue;
            }
        };
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
            replacements += 1;
            replacement_ms += started.map_or(0.0, |s| s.elapsed().as_secs_f64() * 1000.0);
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
        dressed.worn = worn.clone();
        replacements += 1;
        replacement_ms += started.map_or(0.0, |s| s.elapsed().as_secs_f64() * 1000.0);
    }
    stats.replacements += replacements;
    crate::profile::outfit_replacements(replacements, replacement_ms);
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

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_actor::records::{Armor, Npc, Race, RacePart, Weapon};
    use fnv_sim::forms::{ActorBase, BaseData, BaseForm, Entry};
    use fnv_sim::inventory::{Extra, Inventory};
    use bevy::ecs::system::RunSystemOnce;

    const ARMOUR: FormId = FormId(1);
    const WEAPON: FormId = FormId(2);
    const OTHER: FormId = FormId(3);

    fn forms() -> fnv_actor::ActorForms {
        let mut actors = fnv_actor::ActorForms::default();
        actors.armors.insert(ARMOUR, Armor {
            edid: None, slots: fnv_actor::resolve::slot::UPPER_BODY, worn: [None, None],
            dt: 0.0, dr_raw: 0, power_armor: false, ragdoll_template: None, addons: vec![],
        });
        actors.weapons.insert(WEAPON, Weapon {
            edid: None, model: None, anim_type: None, damage: 0, projectile: None,
            explosion_damage: 0.0, embedded: false, non_playable: false, not_for_npcs: false,
            first_person: None, first_person_model: None, mod_models: Default::default(),
            mod_first_person: Default::default(), mod_first_person_models: Default::default(),
            held_world_model: None, grip: None, hide_backpack: true,
        });
        actors
    }

    fn appearance_world() -> ActorWorld {
        let mut actors = forms();
        actors.npcs.insert(OTHER, Npc {
            id: OTHER, edid: None, name: None, female: false, chargen_preset: false,
            template: None, template_flags: 0, skeleton: None, race: Some(OTHER),
            hair: None, hair_length: None, hair_color: None, eyes: None, head_parts: vec![],
            facegen: None, height: None,
        });
        actors.races.insert(OTHER, Race {
            id: OTHER, edid: None, height: [1.0; 2], head: Default::default(),
            body: [vec![RacePart { index: 0, model: Some("bare.nif".into()), icon: None }], vec![]],
            default_hair: [None; 2], facegen: [None, None], name: None, playable: false,
            child: false, hairs: vec![], eyes: vec![], older: None, younger: None, voices: [None; 2],
        });
        let pack = FormId(4);
        let armour = actors.armors.get_mut(&ARMOUR).unwrap();
        armour.worn[0] = Some("armour.nif".into());
        armour.addons.push(pack);
        let mut addon = armour.clone();
        addon.slots = fnv_actor::resolve::slot::BACKPACK;
        addon.worn[0] = Some("pack.nif".into());
        addon.addons.clear();
        actors.armors.insert(pack, addon);
        let weapon = actors.weapons.get_mut(&WEAPON).unwrap();
        weapon.model = Some("weapon.nif".into());
        weapon.mod_models[4] = Some("modded.nif".into());
        ActorWorld {
            actors, forms: Default::default(), facegen_enabled: false, facegen: Default::default(),
            race_facegen: false, show_disabled: false, pose: crate::actors::ActorPose::Bind,
            pose_time: None, walk_test: None, weapon_mode: crate::weapons::WeaponMode::Holstered,
            weapon_test: None, anims: Default::default(), rolls: Default::default(),
            changes: Default::default(), levels: Default::default(),
        }
    }

    fn actor_forms(entries: Vec<Entry>) -> fnv_sim::FormStore {
        let mut forms = fnv_sim::FormStore::default();
        forms.sigs.insert(OTHER, *b"NPC_");
        forms.bases.insert(OTHER, BaseForm {
            id: OTHER, sig: *b"NPC_", edid: None, name: None, script: None, prompt: None,
            voice_filter: false, data: BaseData::Actor(ActorBase { entries, attributes: None }),
        });
        forms
    }

    fn equipped() -> Inventory {
        let mut inv = Inventory::new();
        inv.add(ARMOUR, 1);
        inv.set_equipped(ARMOUR);
        inv.add_extra(WEAPON, 1, Extra { mods: 5, ..Default::default() });
        inv.set_equipped(WEAPON);
        inv
    }

    #[test]
    fn private_world_prefetch_matches_resident_spawn_even_with_a_shared_world() {
        let world = appearance_world();
        let forms = actor_forms(vec![]);
        let refs = fnv_sim::RefTable::default();
        let mut private = fnv_sim::World::new(&forms, &refs, Default::default());
        let mut shared = fnv_sim::World::new(&forms, &refs, Default::default());
        let id = private.place(OTHER, None, [0.0; 3], [0.0; 3], 1).unwrap();
        assert_eq!(shared.place(OTHER, None, [0.0; 3], [0.0; 3], 1).unwrap(), id);
        private.get(id).unwrap().inventory = Some(equipped());
        let authoritative = crate::residents::equipment_world(Some(&private), Some(&shared));
        let plan = crate::readahead::actor_plan(&world, authoritative, id.0, OTHER.0, false).unwrap();
        assert_eq!(plan, crate::readahead::actor_plan(&world, Some(&private), id.0, OTHER.0, false).unwrap());
        assert_ne!(plan, crate::readahead::actor_plan(&world, Some(&shared), id.0, OTHER.0, false).unwrap());
        assert!(plan.parts.iter().any(|p| p.ends_with("modded.nif")));
        assert!(std::ptr::eq(crate::residents::equipment_world(None, Some(&shared)).unwrap(), &shared));
    }

    #[test]
    fn equipment_acquired_after_prefetch_refreshes_the_spawn_plan() {
        let mut world = appearance_world();
        world.facegen_enabled = true;
        world.actors.weapons.get_mut(&WEAPON).unwrap().anim_type = Some("Rifle - Ballistic (2 Hand)".into());
        let forms = actor_forms(vec![]);
        let refs = fnv_sim::RefTable::default();
        let mut sim = fnv_sim::World::new(&forms, &refs, Default::default());
        let id = sim.place(OTHER, None, [0.0; 3], [0.0; 3], 1).unwrap();
        sim.get(id).unwrap().inventory = None;
        let before = crate::readahead::actor_plan(&world, Some(&sim), id.0, OTHER.0, false).unwrap();
        sim.get(id).unwrap().inventory = Some(equipped());
        let after = crate::readahead::actor_plan(&world, Some(&sim), id.0, OTHER.0, false).unwrap();
        assert_ne!(before, after);
        assert_eq!((before.prefix, after.prefix), (Some("h2h"), Some("2hr")));
        assert!(after.facegen.iter().any(|p| p.ends_with("armour.egm")));
        assert!(!after.facegen.iter().any(|p| p.ends_with("bare.egm")));
        let e = Equipment::of(&world, Some(&sim), id.0).unwrap();
        let a = appearance(&world, OTHER.0, id.0, Some(&e)).unwrap();
        assert!(a.parts.iter().all(|p| after.parts.contains(&p.model)));
        assert_eq!((a.weapon, a.weapon_mods), (Some(WEAPON), 5));
    }

    /// A one-node NIF made from the same documented synthetic layout as
    /// `crates/fnv-nif/tests/synthetic.rs`; no game data or original binaries.
    fn skeleton_nif() -> Vec<u8> {
        fn words(out: &mut Vec<u8>, values: &[u32]) { out.extend(values.iter().flat_map(|v| v.to_le_bytes())); }
        fn string(out: &mut Vec<u8>, s: &str) { words(out, &[s.len() as u32]); out.extend(s.as_bytes()); }
        let mut node = vec![];
        words(&mut node, &[0, 0, u32::MAX, 14]); // name, extras, controller, flags
        node.extend([0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0].iter().flat_map(|v| v.to_le_bytes()));
        words(&mut node, &[0, u32::MAX, 0, 0]); // properties, collision, children, effects
        let mut out = b"Gamebryo File Format, Version 20.2.0.7\n".to_vec();
        words(&mut out, &[0x1402_0007]);
        out.push(1);
        words(&mut out, &[11, 1, 34]);
        out.extend([1, 0, 1, 0, 1, 0]); // three empty export strings
        out.extend(1u16.to_le_bytes());
        string(&mut out, "NiNode");
        out.extend(0u16.to_le_bytes());
        words(&mut out, &[node.len() as u32, 1, 4]);
        string(&mut out, "Root");
        words(&mut out, &[0]); // groups
        out.extend(node);
        words(&mut out, &[1, 0]); // footer roots
        out
    }

    struct TestFiles(std::path::PathBuf);
    impl Drop for TestFiles {
        fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
    }

    fn spawn_fixture() -> (App, Entity, TestFiles) {
        use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
        use nvf_assets::{GameData, NifMaterial};
        use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = TestFiles(std::env::temp_dir().join(format!("nvf-spawn-equipment-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed))));
        std::fs::create_dir_all(dir.0.join("meshes")).unwrap();
        for name in ["skeleton", "armour", "bare", "pack", "weapon", "modded"] {
            std::fs::write(dir.0.join("meshes").join(format!("{name}.nif")), skeleton_nif()).unwrap();
        }
        let data = GameData {
            vfs: Arc::new(fnv_vfs::Vfs::open(&dir.0, &[]).unwrap()),
            schema: Arc::new(fnv_nif::Schema::load_dir(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/nif")).unwrap()),
        };
        let mut world = appearance_world();
        world.actors.npcs.get_mut(&OTHER).unwrap().skeleton = Some("skeleton.nif".into());
        let record_weapon = FormId(5);
        let mut record = world.actors.weapons[&WEAPON].clone();
        record.anim_type = Some("Pistol - Ballistic (1 Hand)".into());
        world.actors.weapons.insert(record_weapon, record);
        world.forms = actor_forms(vec![Entry::new(record_weapon, 1)]);
        assert_eq!(world.resolve(OTHER.0, 100).unwrap().weapon, Some(record_weapon));
        let forms = Box::leak(Box::new(actor_forms(vec![])));
        let refs = Box::leak(Box::new(fnv_sim::RefTable::default()));
        let mut sim = SimState { forms, refs, world: fnv_sim::World::new(forms, refs, Default::default()) };
        let id = sim.world.place(OTHER, None, [0.0; 3], [0.0; 3], 1).unwrap();
        sim.world.get(id).unwrap().inventory = Some(equipped());
        let mut app = App::new();
        app.insert_resource(world).insert_resource(sim).insert_resource(data)
            .init_resource::<ActorParts>().init_resource::<OutfitStats>()
            .init_resource::<Assets<Mesh>>().init_resource::<Assets<Image>>()
            .init_resource::<Assets<NifMaterial>>().init_resource::<Assets<SkinnedMeshInverseBindposes>>()
            .init_resource::<nvf_assets::model::ModelCache>().init_resource::<nvf_assets::TextureCache>();
        let parent = app.world_mut().spawn_empty().id();
        let drawn = app.world_mut().run_system_once(move |mut commands: Commands, world: Res<ActorWorld>, sim: Res<SimState>, mut parts: ResMut<ActorParts>, mut models: ModelCtx| {
            let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
            crate::actors::spawn(&mut commands, parent, bevy::math::Affine3A::IDENTITY, Vec3::ZERO, id.0, OTHER.0, &world, Some(&sim.world), &mut ctx, &mut ActorStats::default(), false)
        }).unwrap();
        assert!(drawn);
        let w = app.world_mut();
        let actor = w.query_filtered::<Entity, With<Dressed>>().single(w).unwrap();
        (app, actor, dir)
    }

    #[test]
    fn npc_spawned_from_equipment_reconciles_without_rebuilding() {
        let (mut app, actor, _files) = spawn_fixture();
        assert_eq!(app.world().get::<Dressed>(actor).unwrap().weapon, Some(WEAPON));
        let before = app.world().get::<Dressed>(actor).unwrap().clone();
        app.world_mut().run_system_once(follow_inventory).unwrap();
        let after = app.world().get::<Dressed>(actor).unwrap();
        assert_eq!((&after.worn, after.weapon, after.weapon_mods, &after.parts), (&before.worn, before.weapon, before.weapon_mods, &before.parts));
        let stats = app.world().resource::<OutfitStats>();
        assert_eq!(stats.replacements, 0);
        assert_eq!((stats.armour_off, stats.armour_on, stats.weapons_off, stats.weapons_on, stats.parts_on), (0, 0, 0, 0, 0));
    }

    #[test]
    fn corpse_keeps_its_simulated_weapon_and_later_changes_do_not_rearm_it() {
        let (mut app, actor, _files) = spawn_fixture();
        let rig = app.world().get::<ActorRig>(actor).unwrap().clone();
        // Death presentation follows spawn. Its weapon differs from the record's,
        // but `combat::death_drops` intentionally keeps dead-at-spawn weapons.
        app.world_mut().entity_mut(actor).insert((Corpse::default(), BlendPlayer::new(rig.nodes.clone(), std::sync::Arc::new(rig.skel.locals.clone()))));
        let held = app.world_mut().spawn(HeldWeapon { reference: rig.reference, weapon: WEAPON }).id();
        app.world_mut().run_system_once(follow_inventory).unwrap();
        assert!(app.world().get_entity(held).is_ok());
        assert_eq!(app.world().resource::<OutfitStats>().replacements, 0);
        assert_eq!(app.world().get::<Dressed>(actor).unwrap().weapon, Some(WEAPON));
        // A later Unequip/loot still removes that weapon without arming the corpse.
        app.world_mut().resource_mut::<SimState>().world.get(FormId(rig.reference)).unwrap().inventory = Some(Inventory::new());
        app.world_mut().run_system_once(follow_inventory).unwrap();
        assert!(app.world().get_entity(held).is_err());
        assert!(app.world().get::<WeaponHold>(actor).is_none());
        assert_eq!(app.world().resource::<OutfitStats>().weapons_on, 0);
        assert_eq!(app.world().get::<Dressed>(actor).unwrap().weapon, None);
    }

    #[test]
    fn simulated_equipment_selects_unique_armour_and_the_held_units_mods() {
        let actors = forms();
        let selected = Equipment::select(&actors, Some(&[OTHER, ARMOUR, WEAPON, ARMOUR]), |w| {
            assert_eq!(w, WEAPON);
            5
        });
        assert_eq!(selected, Some(Equipment { worn: vec![ARMOUR], weapon: Some(WEAPON), weapon_mods: 5 }));
    }

    #[test]
    fn unavailable_simulation_equipment_preserves_record_fallback() {
        assert_eq!(Equipment::select(&forms(), None, |_| panic!("no held unit")), None);
    }

    #[test]
    fn partial_equipment_does_not_fill_missing_slots_from_the_record() {
        let actors = forms();
        assert_eq!(Equipment::select(&actors, Some(&[ARMOUR]), |_| panic!("no held unit")),
            Some(Equipment { worn: vec![ARMOUR], weapon: None, weapon_mods: 0 }));
        assert_eq!(Equipment::select(&actors, Some(&[WEAPON]), |_| 3),
            Some(Equipment { worn: vec![], weapon: Some(WEAPON), weapon_mods: 3 }));
        for items in [&[][..], &[OTHER][..]] {
            assert_eq!(Equipment::select(&actors, Some(items), |_| panic!("no held unit")),
                Some(Equipment { worn: vec![], weapon: None, weapon_mods: 0 }));
        }
    }

    #[test]
    fn shared_appearance_resolves_parts_weapon_mods_and_hide_backpack() {
        let world = appearance_world();
        assert_eq!(appearance(&world, OTHER.0, 100, None), world.resolve(OTHER.0, 100));
        let equipment = Equipment::select(&world.actors, Some(&[ARMOUR, WEAPON]), |_| 5).unwrap();
        let a = appearance(&world, OTHER.0, 100, Some(&equipment)).unwrap();
        assert_eq!((a.worn.as_slice(), a.weapon, a.weapon_mods), (&[ARMOUR][..], Some(WEAPON), 5));
        assert!(a.parts.iter().any(|p| p.model.ends_with("armour.nif")));
        assert!(!a.parts.iter().any(|p| p.model.ends_with("bare.nif") || p.model.ends_with("pack.nif")));
        assert_eq!(world.actors.weapons[&a.weapon.unwrap()].model_for(false, a.weapon_mods), Some("modded.nif"));
        let unarmed = Equipment::select(&world.actors, Some(&[ARMOUR]), |_| unreachable!()).unwrap();
        let a = appearance(&world, OTHER.0, 100, Some(&unarmed)).unwrap();
        assert!(a.parts.iter().any(|p| p.model.ends_with("pack.nif")));
        assert_eq!((a.weapon, a.weapon_mods), (None, 0));
        let empty = Equipment::select(&world.actors, Some(&[]), |_| unreachable!()).unwrap();
        let a = appearance(&world, OTHER.0, 100, Some(&empty)).unwrap();
        assert!(a.worn.is_empty());
        assert!(a.parts.iter().any(|p| p.model.ends_with("bare.nif")));
    }

    #[test]
    fn simulation_lookup_and_prefetch_use_the_same_equipment() {
        use fnv_sim::forms::{ActorBase, BaseData, BaseForm};
        use fnv_sim::inventory::{Extra, Inventory};
        let mut world = appearance_world();
        world.forms.sigs.insert(OTHER, *b"NPC_");
        world.forms.bases.insert(OTHER, BaseForm {
            id: OTHER, sig: *b"NPC_", edid: None, name: None, script: None, prompt: None,
            voice_filter: false, data: BaseData::Actor(ActorBase { entries: vec![], attributes: None }),
        });
        let refs = fnv_sim::RefTable::default();
        let mut sim = fnv_sim::World::new(&world.forms, &refs, Default::default());
        assert_eq!(Equipment::of(&world, None, 100), None);
        assert_eq!(Equipment::of(&world, Some(&sim), 100), None);
        let id = sim.place(OTHER, None, [0.0; 3], [0.0; 3], 1).unwrap();
        sim.get(id).unwrap().inventory = None;
        assert_eq!(Equipment::of(&world, Some(&sim), id.0), None);
        let mut inv = Inventory::new();
        inv.add(ARMOUR, 1);
        inv.set_equipped(ARMOUR);
        sim.get(id).unwrap().inventory = Some(inv);
        assert_eq!(Equipment::of(&world, Some(&sim), id.0),
            Some(Equipment { worn: vec![ARMOUR], weapon: None, weapon_mods: 0 }));
        let inv = sim.get(id).unwrap().inventory.as_mut().unwrap();
        inv.add_extra(WEAPON, 1, Extra { mods: 5, ..Default::default() });
        inv.set_equipped(WEAPON);
        assert_eq!(Equipment::of(&world, Some(&sim), id.0),
            Some(Equipment { worn: vec![ARMOUR], weapon: Some(WEAPON), weapon_mods: 5 }));
        let plan = crate::readahead::actor_plan(&world, Some(&sim), id.0, OTHER.0, false).unwrap();
        assert!(plan.parts.iter().any(|p| p.ends_with("armour.nif")));
        assert!(plan.parts.iter().any(|p| p.ends_with("modded.nif")));
        assert!(!plan.parts.iter().any(|p| p.ends_with("bare.nif") || p.ends_with("pack.nif")));
    }
}
