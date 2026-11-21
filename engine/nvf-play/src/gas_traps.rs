//! Gas traps: the clouds of leaking gas that shots, blasts and guns fired inside set
//! alight (#2687).
//!
//! A gas cloud is a placed activator such as `TrapGasFire01` or Vault 22's
//! `V22TrapGasFire01` (FalloutNV.esm ACTI 0x000348D1, 0x000EF40A). Its model,
//! `meshes\traps\trapgasfire01.nif`, has nothing solid in it. Its only collision is a
//! sphere phantom on the Gas Trap layer (24) [observed: `fnvtool nif decode`]. The
//! record's destruction data does the burning: health 10; stage 1 at 99% (Cap Damage,
//! Destroy, 50 self-damage a second) burns it down, and stage 2 at 0% (Disable)
//! sets off explosion 0x000348D0 [observed: `fnvtool esm decode`]. The scripts
//! (`TrapGasSCRIPT` 0x00075333, `V22TrapGasSCRIPT` 0x000EF405) do `DamageObject 5` in
//! `OnHitWith ExplodesGasTrapsOnHit` (FLST 0x0007627F: the energy, fire and
//! electric weapons, the launchers, dynamite and `GasTrapDummy`), in
//! `OnMagicEffectHit FlamerMagicEffect`, and in `OnTriggerEnter` for an actor with a
//! weapon of `ExplodesGasTrapsOnTouch` drawn or burning [observed: `fnvtool script
//! decompile`]. Vault 22's spore purge (There Stands the Grass, VMS06 objective 50)
//! is completed only by such a cloud's `OnDestructionStageChange`.
//!
//! How the engine reaches them [documented: GECK wiki Traps, a gas trap goes off when
//! hit by a weapon in `ExplodesGasTrapsOnHit`, by explosions, when touched with a weapon
//! in `ExplodesGasTrapsOnTouch` or while burning, or when a weapon is fired inside it,
//! and neighbouring gas traps set each other off; docs/spec-notes/level-design.md §11]:
//! - **Walking in**: the clouds' volumes are trigger volumes (`crate::script_trigger`),
//!   so their OnTriggerEnter runs.
//! - **A shot's path** through a cloud is a hit on it with the shot's weapon: its
//!   OnHit and OnHitWith blocks, then OnMagicEffectHit for the weapon's object effect
//!   (`crate::combat`). The path may also stop on something behind the cloud, or inside it.
//! - **A gun fired inside** a cloud, and **a blast** that reaches one, also hit it with
//!   `GasTrapDummy` (WEAP 0x000001F6) when the weapon's own hit ran none of its blocks.
//!   This covers a 10mm pistol, which is in neither list, and a frag grenade
//!   [hypothesis: the engine makes these hits with the dummy weapon. That weapon has no
//!   model, projectile or damage, sits in `ExplodesGasTrapsOnHit` and carries a FormID
//!   below 0x800, as forms the engine names itself do; observer check pending, #2687].
//!   Through the burning cloud's own explosion, a blast also chains to the clouds beside it.

use crate::script::Runtime;
use crate::script_events::TriggerShape;
use fnv_sim::FormId;

/// `GasTrapDummy`, the weapon the engine's own hits on gas clouds carry (module notes).
pub const GAS_TRAP_DUMMY: FormId = FormId(0x0000_01F6);

/// The gas clouds that can still go off now: present, not destroyed yet, in a loaded
/// cell, with their volumes. Call it with the world in the runtime
/// (`crate::script::with_world`).
pub fn volumes(rt: &Runtime) -> Vec<(FormId, TriggerShape)> {
    let g = &rt.game;
    rt.triggers
        .gas_volumes()
        .filter(|(r, _)| g.present(*r) && !g.extra.get(r).is_some_and(|e| e.destroyed) && g.cell_of(*r).is_some_and(|c| g.loaded_cells.contains(&c)))
        .map(|(r, s)| (r, *s))
        .collect()
}

/// Each cloud once, in the order first met.
fn each_once(it: impl Iterator<Item = FormId>) -> Vec<FormId> {
    let mut out: Vec<FormId> = Vec::new();
    for r in it {
        if !out.contains(&r) {
            out.push(r);
        }
    }
    out
}

/// The clouds the path `a`–`b` passes through.
pub fn crossed(vols: &[(FormId, TriggerShape)], a: [f32; 3], b: [f32; 3]) -> Vec<FormId> {
    each_once(vols.iter().filter(|(_, s)| s.crossed_by(a, b)).map(|(r, _)| *r))
}

/// The clouds holding the point `p` (a muzzle).
pub fn around(vols: &[(FormId, TriggerShape)], p: [f32; 3]) -> Vec<FormId> {
    each_once(vols.iter().filter(|(_, s)| s.contains(p)).map(|(r, _)| *r))
}

/// The clouds a blast at `at` of `radius` reaches.
pub fn in_blast(vols: &[(FormId, TriggerShape)], at: [f32; 3], radius: f32) -> Vec<FormId> {
    each_once(vols.iter().filter(|(_, s)| s.overlaps_ball(at, radius)).map(|(r, _)| *r))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script_events::TriggerKind;

    const IDENTITY: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

    fn cloud(at: [f32; 3], radius: f32) -> TriggerShape {
        TriggerShape { pos: at, rot: IDENTITY, scale: 1.0, kind: TriggerKind::Sphere { radius } }
    }

    /// #2687: a laser shot through Vault 22's purge gas crosses it; one beside it
    /// does not; a muzzle inside it is inside; a blast at its edge reaches it.
    #[test]
    fn shots_and_blasts_find_the_gas_clouds_they_reach() {
        let purge = FormId(0x000F_0908);
        let other = FormId(0x000F_98F7);
        let vols = vec![(purge, cloud([0.0, 0.0, 0.0], 128.0)), (other, cloud([1000.0, 0.0, 0.0], 128.0))];
        assert_eq!(crossed(&vols, [-500.0, 0.0, 0.0], [500.0, 0.0, 0.0]), vec![purge], "through the middle, short of the second");
        assert_eq!(crossed(&vols, [-500.0, 0.0, 0.0], [1500.0, 0.0, 0.0]), vec![purge, other]);
        assert!(crossed(&vols, [-500.0, 200.0, 0.0], [500.0, 200.0, 0.0]).is_empty(), "passes beside it");
        assert!(crossed(&vols, [-500.0, 0.0, 0.0], [-200.0, 0.0, 0.0]).is_empty(), "stops short of it");
        assert_eq!(crossed(&vols, [10.0, 10.0, 0.0], [20.0, 10.0, 0.0]), vec![purge], "wholly inside");
        assert_eq!(around(&vols, [50.0, 0.0, 30.0]), vec![purge]);
        assert!(around(&vols, [500.0, 0.0, 0.0]).is_empty());
        assert_eq!(in_blast(&vols, [300.0, 0.0, 0.0], 200.0), vec![purge], "reaches its edge");
        assert!(in_blast(&vols, [500.0, 0.0, 0.0], 200.0).is_empty());
        assert_eq!(in_blast(&vols, [500.0, 0.0, 0.0], 400.0), vec![purge, other]);
    }

    #[test]
    fn a_box_volume_is_crossed_by_a_path_through_its_faces() {
        let b = TriggerShape { pos: [0.0, 0.0, 0.0], rot: IDENTITY, scale: 2.0, kind: TriggerKind::Box { half_extents: [10.0, 10.0, 10.0] } };
        assert!(b.crossed_by([-100.0, 15.0, 0.0], [100.0, 15.0, 0.0]), "scaled: 20 units half extents");
        assert!(!b.crossed_by([-100.0, 25.0, 0.0], [100.0, 25.0, 0.0]));
        assert!(b.crossed_by([-100.0, -100.0, 0.0], [100.0, 100.0, 0.0]), "diagonal");
        assert!(!b.crossed_by([30.0, 0.0, 0.0], [100.0, 0.0, 0.0]), "beyond it");
    }
}
