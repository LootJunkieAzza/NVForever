//! Candidate-only internal queue/lifetime controls; not baseline instrumentation.
use super::*;

#[test]
fn successful_source_positions_survive_missing_consumer_but_not_restore_or_retirement() {
    let mut a = lifecycle_app(0, 1, 5000.0);
    let (r, id) = launch(&mut a, "OwnedGun");
    on(&mut a, r, "subject.SetPos Z 999");
    assert_eq!(a.world().resource::<ScriptState>().rt.as_ref().unwrap().game.gaps.projs.position_sets[&r][2], 999.0);
    let state = a.world_mut().remove_resource::<ScriptState>().unwrap();
    let before = a.world().resource::<Combat>().flying.iter().find(|f| f.rref == Some(r)).unwrap().p.pos;
    step(&mut a, 1.0 / 60.0);
    assert_ne!(a.world().resource::<Combat>().flying.iter().find(|f| f.rref == Some(r)).unwrap().p.pos, before, "no runtime is not a deletion or pose command");
    a.insert_resource(state);
    step(&mut a, 0.0);
    assert_eq!(a.world().resource::<Combat>().flying.iter().find(|f| f.rref == Some(r)).unwrap().p.pos[2], 999.0);
    assert!(a.world().resource::<ScriptState>().rt.as_ref().unwrap().game.gaps.projs.position_sets.is_empty());
    on(&mut a, r, "subject.SetPos Z 777\nsubject.MarkForDelete");
    step(&mut a, 0.0);
    retired(&a, r, id);
    assert!(a.world().resource::<ScriptState>().rt.as_ref().unwrap().game.gaps.projs.position_sets.is_empty());

    let (r, _) = launch(&mut a, "OwnedGun");
    on(&mut a, r, "subject.SetPos Z 555");
    a.world_mut().resource_scope(|world, mut state: Mut<ScriptState>| {
        let mut sim = world.resource_mut::<SimState>();
        let rt = state.rt.as_mut().unwrap();
        let snapshot = with_world(rt, &mut sim, |rt| rt.game.checked_snapshot()).unwrap();
        rt.restore_save(&mut sim, snapshot).unwrap();
        assert!(rt.game.gaps.projs.shots.is_empty(), "existing temporary shot session is not restored");
        assert!(rt.game.gaps.projs.position_sets.is_empty(), "queued positions are not new save data");
    });
}
