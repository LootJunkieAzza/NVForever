//! Source calls and actual emitted flights; no injected Flying/Pending/Settled events.
use super::*;
use bevy::ecs::system::RunSystemOnce;

fn owner(a: &mut App) -> FormId {
    match query(a, "SetFunctionValue (Player.PlaceAtMe 0x00000007 1)") {
        XVal::Ref(r) if r != 0 => FormId(r),
        x => panic!("actual created owner reference: {x:?}"),
    }
}

fn close(actual: V3, expected: V3) {
    for i in 0..3 { assert!((actual[i] - expected[i]).abs() < 1e-3, "{actual:?} != {expected:?}"); }
}

#[test]
fn source_setpos_moves_real_flight_and_visual_once_without_restarting_it_or_neighbor() {
    let mut a = lifecycle_app(0, 1, 5000.0);
    let (r, id) = launch(&mut a, "OwnedGun");
    let (neighbor, neighbor_id) = launch(&mut a, "NeighborGun");
    let c = a.world().resource::<Combat>();
    let f = c.flying.iter().find(|f| f.rref == Some(r)).unwrap();
    let (before, velocity, age, distance, timer, origin, seen, shooter) = (f.p.pos, f.p.vel, f.p.age, f.p.travelled, f.p.timer, f.origin, f.seen, f.p.shooter);
    let next = [before[0] + 120.0, before[1] + 320.0, before[2] + 100.0];
    let neighbor_before = c.flying.iter().find(|f| f.rref == Some(neighbor)).unwrap().p.pos;
    on(&mut a, r, &format!("subject.SetPos X {}\nsubject.SetPos Y {}\nsubject.SetPos Z {}", next[0], next[1], next[2]));
    assert_eq!(a.world().resource::<SimState>().world.peek(r).unwrap().pos, next);
    step(&mut a, 0.0);
    let c = a.world().resource::<Combat>();
    let f = c.flying.iter().find(|f| f.rref == Some(r)).unwrap();
    assert_eq!(f.p.pos, next, "canonical world edit must reach actual physical flight");
    assert_eq!((f.p.vel, f.p.age, f.p.travelled, f.p.timer, f.origin, f.seen, f.p.shooter), (velocity, age, distance, timer, origin, seen, shooter));
    assert_eq!(c.flying.iter().find(|f| f.rref == Some(neighbor)).unwrap().p.pos, neighbor_before);
    assert_eq!(a.world().resource::<ShotVisuals>().flying.iter().find(|v| v.id == id).unwrap().pos, seen.at(next, distance));
    assert!(a.world().resource::<ShotVisuals>().flying.iter().any(|v| v.id == neighbor_id));
    assert_eq!(on(&mut a, r, "SetFunctionValue (subject.GetPos Z)"), XVal::Num(f64::from(next[2])));
    for n in 1..=2 {
        step(&mut a, 1.0 / 60.0);
        let c = a.world().resource::<Combat>();
        let f = c.flying.iter().find(|f| f.rref == Some(r)).unwrap();
        close(f.p.pos, vec::add(next, vec::scale(velocity, n as f32 / 60.0)));
        assert!(f.p.age > age && f.p.travelled > distance, "consumed command does not rewind each frame");
    }
}

#[test]
fn changed_source_reaches_real_blast_attacker_and_effect_without_changing_collision_shooter() {
    let mut explosion = vec![0; 52];
    // Public ExplosionData schema: force, damage, radius, light, sound, flags.
    explosion[4..8].copy_from_slice(&20f32.to_le_bytes());
    explosion[8..12].copy_from_slice(&512f32.to_le_bytes());
    explosion[20..24].copy_from_slice(&16u32.to_le_bytes()); // Ignore LOS Check
    let flags = 2 | 4;
    let mut a = lifecycle_app_from(lifecycle_plugin_with_explosion(flags, 1, 5000.0, explosion), flags);
    a.world_mut().resource_scope(|_, mut phys: Mut<PhysState>| {
        let body = phys.world.add_character(0, Default::default(), [10.0, 20.0, 30.0]);
        phys.player = Some(body);
    });
    let changed_owner = owner(&mut a);
    let victim = owner(&mut a);
    on(&mut a, victim, "subject.SetPos X 100");
    // ActorHit is the NPC consumer; the player has its own hit feedback path.
    // Keep the victim away from the shot line so only the real blast reaches it.
    a.world_mut().resource_mut::<PhysState>().world.set_actor_volume(victim.0,
        fnv_phys::HitVolume { feet: [100.0, 20.0, 30.0], radius: 16.0, height: 128.0, dead: false });
    let (r, id) = launch(&mut a, "OwnedGun");
    let (neighbor, _) = launch(&mut a, "NeighborGun");
    let shooter = a.world().resource::<Combat>().flying.iter().find(|f| f.rref == Some(r)).unwrap().p.shooter;
    on(&mut a, r, &format!("ref newOwner\nlet newOwner := StringToRef \"{:08X}\"\nsubject.SetProjectileRefSource newOwner", changed_owner.0));
    step(&mut a, 0.0);
    let f = a.world().resource::<Combat>().flying.iter().find(|f| f.rref == Some(r)).unwrap();
    assert_eq!(f.p.shooter, shooter, "attacker identity does not invent collision-exclusion semantics");
    let published_owner = a.world().resource::<ShotVisuals>().flying.iter().find(|v| v.id == id).unwrap().owner;
    a.world_mut().resource_mut::<Messages<ActorHit>>().drain().for_each(drop);
    a.world_mut().resource_mut::<Messages<ExplosionFx>>().drain().for_each(drop);
    let player_hits_before = a.world().resource::<CombatStats>().player_hits;
    on(&mut a, r, "subject.SetDetonationTimer 0");
    step(&mut a, 1.0 / 60.0);
    let effects: Vec<_> = a.world_mut().resource_mut::<Messages<ExplosionFx>>().drain().collect();
    assert!(effects.iter().any(|e| e.explosion == BLAST && e.cause == Some(changed_owner)), "real emitted blast retains the changed source: {effects:?}");
    let hits: Vec<_> = a.world_mut().resource_mut::<Messages<ActorHit>>().drain().collect();
    assert!(hits.iter().any(|h| h.form_id == victim.0 && h.by == Some(changed_owner)), "actual blast hit uses the new attacker: {hits:?}");
    assert!(a.world().resource::<CombatStats>().player_hits > player_hits_before, "the real blast also reaches the existing player damage consumer");
    assert_eq!(published_owner, changed_owner, "published flight ownership agrees with the actual blast");
    assert_eq!(a.world().resource::<CombatStats>().explosions, 1);
    assert!(a.world().resource::<Combat>().flying.iter().any(|f| f.rref == Some(neighbor)));
}

#[test]
fn changed_source_reaches_an_actual_landed_mines_world_owner() {
    fn tune(nodes: &mut [fnv_esm::Node]) {
        for node in nodes {
            match node {
                fnv_esm::Node::Group(g) => tune(&mut g.children),
                fnv_esm::Node::Record(r) if r.header.sig == *b"PROJ" => {
                    let mut subs = r.subrecords().unwrap();
                    let data = &mut subs.iter_mut().find(|s| s.sig == *b"DATA").unwrap().data;
                    data[4..8].copy_from_slice(&1f32.to_le_bytes()); // schema gravity
                    data[28..32].copy_from_slice(&64f32.to_le_bytes()); // schema proximity: mine
                    r.raw = write_subrecords(&subs).into();
                }
                _ => {}
            }
        }
    }
    let flags = 2 | 4;
    let mut plugin = lifecycle_plugin(flags, 2, 5000.0);
    tune(&mut plugin.nodes);
    let mut a = lifecycle_app_from(plugin, flags);
    a.init_resource::<crate::mines::Mines>();
    ground_and_down(&mut a);
    let changed_owner = owner(&mut a);
    let (r, _) = launch(&mut a, "OwnedGun");
    assert!(a.world().resource::<Combat>().flying.iter().find(|f| f.rref == Some(r)).unwrap().p.mine);
    on(&mut a, r, &format!("ref newOwner\nlet newOwner := StringToRef \"{:08X}\"\nsubject.SetProjectileRefSource newOwner", changed_owner.0));
    for _ in 0..600 {
        step(&mut a, 1.0 / 60.0);
        if !a.world().resource::<Combat>().flying.iter().any(|f| f.rref == Some(r)) { break; }
    }
    assert!(!a.world().resource::<Combat>().flying.iter().any(|f| f.rref == Some(r)), "real terrain contact must settle the emitted mine");
    assert!(a.world().resource::<Combat>().settled.iter().any(|s| s.proj == PROJ), "actual Projectile::step produced a settled event");
    a.world_mut().run_system_once(crate::mines::tick).unwrap();
    let sim = a.world().resource::<SimState>();
    let placed: Vec<_> = sim.world.created().filter(|(_, s)| s.base == PROJ && !s.shot && !s.deleted()).collect();
    assert_eq!(placed.len(), 1, "real settled-mine consumer places one reference");
    assert_eq!(placed[0].1.extra.owner, Some(changed_owner), "landed ownership must use the changed source");
}

#[test]
fn nonfinite_position_does_not_poison_actual_flight() {
    let mut a = lifecycle_app(0, 1, 5000.0);
    let (r, _) = launch(&mut a, "OwnedGun");
    let before = a.world().resource::<Combat>().flying.iter().find(|f| f.rref == Some(r)).unwrap().p.pos;
    on(&mut a, r, "subject.SetPos Z (100000000000000000000.0 * 100000000000000000000.0)");
    assert!(!a.world().resource::<SimState>().world.peek(r).unwrap().pos[2].is_finite(), "canonical conversion/result admission remains unchanged");
    step(&mut a, 0.0);
    assert_eq!(a.world().resource::<Combat>().flying.iter().find(|f| f.rref == Some(r)).unwrap().p.pos, before);
    assert_eq!(a.world().resource::<SimState>().world.peek(r).unwrap().pos, before);
}

#[test]
fn null_owner_is_not_a_player_fallback_and_removed_edited_shot_cannot_blast() {
    let mut a = lifecycle_app(2 | 4, 1, 5000.0);
    let changed_owner = owner(&mut a);
    let (r, id) = launch(&mut a, "OwnedGun");
    let (neighbor, _) = launch(&mut a, "NeighborGun");
    on(&mut a, neighbor, &format!("ref newOwner\nlet newOwner := StringToRef \"{:08X}\"\nsubject.SetProjectileRefSource newOwner\nref absent\nsubject.SetProjectileRefSource absent", changed_owner.0));
    assert_eq!(on(&mut a, neighbor, "SetFunctionValue (subject.GetProjectileRefSource)"), XVal::Ref(changed_owner.0));
    on(&mut a, r, "subject.SetPos Z 999\nsubject.SetDetonationTimer 0\nsubject.MarkForDelete");
    step(&mut a, 1.0 / 60.0);
    retired(&a, r, id);
    assert_eq!(a.world().resource::<CombatStats>().explosions, 0);
    assert!(a.world().resource::<ShotVisuals>().flying.iter().any(|v| v.projectile == NEIGHBOR), "unrelated neighbor remains published");
    // Ordinary references are not admitted as live shots by these APIs.
    assert_eq!(query(&mut a, "Player.SetProjectileRefSource Player\nSetFunctionValue (Player.GetProjectileRefSource)"), XVal::Ref(0));
}

#[test]
fn pending_hits_and_travel_keep_their_existing_policy_after_position_writes() {
    let mut a = lifecycle_app(1, 1, 5000.0);
    ground_and_down(&mut a);
    let (r, _) = launch(&mut a, "OwnedGun");
    let before = {
        let c = a.world().resource::<Combat>();
        assert_eq!(c.pending.len(), 1);
        (c.pending[0].point, c.pending[0].due, c.pending[0].by.actor)
    };
    on(&mut a, r, "subject.SetPos Z 999");
    step(&mut a, 0.0);
    let c = a.world().resource::<Combat>();
    assert_eq!((c.pending[0].point, c.pending[0].due, c.pending[0].by.actor), before, "a resolved hit is not retargeted by a position edit");
    step(&mut a, 0.2);
    assert_eq!(a.world().resource::<CombatStats>().world_hits, 1);

    let mut a = lifecycle_app(0, 1, 5000.0);
    let (r, id) = launch(&mut a, "OwnedGun");
    on(&mut a, r, "subject.SetPos Z 999");
    a.world_mut().resource_mut::<Messages<crate::Travel>>().write(crate::Travel { world: None, interior: Some(CELL.0), feet: [0.0; 3], heading_deg: 0.0, pitch_deg: None });
    a.world_mut().run_system_once(leave_space_shots).unwrap();
    retired(&a, r, id);
    step(&mut a, 0.0);
    assert!(a.world().resource::<Combat>().flying.is_empty());
}
