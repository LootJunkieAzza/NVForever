use super::*;

#[test]
fn exhausted_callable_regions_return_no_handle_before_adding_units_or_lambdas() {
    let (forms, refs, data) = fixture();
    let mut g = Game::new(
        &data,
        World::new(&forms, &refs, Default::default()),
        GameConfig {
            strict: true,
            ..Default::default()
        },
    );
    g.next_compiled = TEMP_REF_BASE;
    assert!(g
        .compile_source("exhausted", "Begin Function {}\nEnd\n".into())
        .is_none());
    assert!(g.compiled_files.is_empty() && g.compiled_slots.is_empty());
    assert_eq!(g.next_compiled, TEMP_REF_BASE);
    assert!(g.events.iter().any(|e| matches!(&e.kind, EventKind::Skipped { what, .. } if what == "exhausted: no compiled-script ID left")), "{:?}", g.events);
    g.next_lambda = FormId::FIRST_DYNAMIC.0;
    let result = g.run_query(
        "Begin Function {}\nSetFunctionValue (Begin Function {}\nEnd)\nEnd\n",
        Default::default(),
    );
    assert_eq!(result, Err("runtime lambda handles exhausted".into()));
    assert!(g.lambdas.is_empty() && g.lambda_slots.is_empty());
    assert!(g.lambda_ids.is_empty());
    assert_eq!(g.next_lambda, FormId::FIRST_DYNAMIC.0);
    assert!(g.compiled_files.is_empty() && g.compiled_slots.is_empty());
    g.other.jip13.made_packages = FormId::FIRST_DYNAMIC.0 - LAMBDA_BASE;
    let result = crate::nvse_jip13::call(
        &mut g,
        &Default::default(),
        "traveltoref",
        Some(FormId::PLAYER_REF),
        &[fnv_script::vm::XVal::Ref(1)],
    );
    assert_eq!(
        result,
        Some(Err("runtime travel package handles exhausted".into()))
    );
    assert!(g.package_overrides.is_empty());
    assert!(
        !g.extra.contains_key(&FormId::PLAYER_REF),
        "capacity failure precedes actor/package mutation"
    );
    assert_eq!(
        g.other.jip13.made_packages,
        FormId::FIRST_DYNAMIC.0 - LAMBDA_BASE
    );
}

#[test]
fn non_strict_lambda_exhaustion_keeps_the_existing_recoverable_error_policy() {
    let (forms, refs, data) = fixture();
    let mut g = Game::new(
        &data,
        World::new(&forms, &refs, Default::default()),
        Default::default(),
    );
    assert!(!g.cfg.strict, "characterize the accepted default mode");
    g.next_lambda = FormId::FIRST_DYNAMIC.0;
    let result = g.run_query(
        "Begin Function {}\nSetFunctionValue (Begin Function {}\nEnd)\nEnd\n",
        Default::default(),
    );
    assert_eq!(result, Ok(fnv_script::vm::XVal::Num(0.0)));
    assert_eq!(g.events.iter().filter(|e| matches!(&e.kind, EventKind::Skipped { what, .. } if what == "runtime lambda handles exhausted")).count(), 1, "{:?}", g.events);
    assert!(g.lambdas.is_empty() && g.lambda_slots.is_empty() && g.lambda_ids.is_empty());
    assert_eq!(g.next_lambda, FormId::FIRST_DYNAMIC.0);
    assert!(g.compiled_files.is_empty() && g.compiled_slots.is_empty());
    assert!(g.udf.is_empty() && g.ext.is_empty());
}

#[test]
fn last_free_handles_keep_cached_lambda_identity_and_refuse_only_new_allocations() {
    use fnv_script::vm::XVal;
    let (forms, refs, data) = fixture();
    let mut g = Game::new(
        &data,
        World::new(&forms, &refs, Default::default()),
        GameConfig {
            strict: true,
            ..Default::default()
        },
    );
    let maker =
        "Begin Function {}\nSetFunctionValue (Begin Function {}\nSetFunctionValue 7\nEnd)\nEnd\n";
    g.next_compiled = TEMP_REF_BASE - 1;
    let compiled = g
        .compile_source("last free unit", maker.into())
        .expect("last compiled slot must remain usable");
    assert_eq!(compiled, TEMP_REF_BASE - 1);
    assert_eq!(g.next_compiled, TEMP_REF_BASE);
    assert!(g.compile_source("past last unit", maker.into()).is_none());
    assert_eq!((g.compiled_files.len(), g.compiled_slots.len()), (1, 1));
    assert_eq!(g.next_compiled, TEMP_REF_BASE);
    g.next_lambda = FormId::FIRST_DYNAMIC.0 - 1;
    let first = g
        .run_function(&XVal::Ref(compiled), None, vec![], None, Default::default())
        .unwrap();
    assert_eq!(first, XVal::Ref(FormId::FIRST_DYNAMIC.0 - 1));
    assert_eq!(g.next_lambda, FormId::FIRST_DYNAMIC.0);
    let cached = g
        .run_function(&XVal::Ref(compiled), None, vec![], None, Default::default())
        .unwrap();
    assert_eq!(
        cached, first,
        "the same maker/instance reuses its existing identity at capacity"
    );
    assert_eq!(
        g.run_function(&cached, None, vec![], None, Default::default())
            .unwrap(),
        XVal::Num(7.0)
    );
    assert_eq!(
        (g.lambdas.len(), g.lambda_slots.len(), g.lambda_ids.len()),
        (1, 1, 1)
    );
    let distinct = g.run_query(maker, Default::default());
    assert_eq!(
        distinct,
        Err("runtime lambda handles exhausted".into()),
        "a different query unit cannot borrow the cached maker's identity"
    );
    assert_eq!(
        (g.lambdas.len(), g.lambda_slots.len(), g.lambda_ids.len()),
        (1, 1, 1)
    );
    assert_eq!(g.next_lambda, FormId::FIRST_DYNAMIC.0);
    assert_eq!((g.compiled_files.len(), g.compiled_slots.len()), (1, 1));

    let mut g = Game::new(
        &data,
        World::new(&forms, &refs, Default::default()),
        Default::default(),
    );
    g.other.jip13.made_packages = FormId::FIRST_DYNAMIC.0 - LAMBDA_BASE - 1;
    let travel = |g: &mut Game<'_>| {
        crate::nvse_jip13::call(
            g,
            &Default::default(),
            "traveltoref",
            Some(FormId::PLAYER_REF),
            &[XVal::Ref(1)],
        )
    };
    assert_eq!(travel(&mut g), Some(Ok(XVal::Num(0.0))));
    let last = FormId(FormId::FIRST_DYNAMIC.0 - 1);
    assert_eq!(g.extra[&FormId::PLAYER_REF].script_package, Some(last));
    assert_eq!(g.package_overrides.len(), 1);
    assert!(g.package_overrides.contains_key(&last));
    let full = g.other.jip13.made_packages;
    assert_eq!(full, FormId::FIRST_DYNAMIC.0 - LAMBDA_BASE);
    // The same journey asked again keeps the package made for it (#3150), so it needs
    // no handle; a different one does, and none is left.
    assert_eq!(travel(&mut g), Some(Ok(XVal::Num(0.0))));
    assert_eq!(g.package_overrides.len(), 1);
    assert_eq!(g.other.jip13.made_packages, full);
    assert_eq!(
        crate::nvse_jip13::call(
            &mut g,
            &Default::default(),
            "traveltoref",
            Some(FormId::PLAYER_REF),
            &[XVal::Ref(1), XVal::Num(1.0)],
        ),
        Some(Err("runtime travel package handles exhausted".into()))
    );
    assert_eq!(g.other.jip13.made_packages, full);
    assert_eq!(g.extra[&FormId::PLAYER_REF].script_package, Some(last));
    assert_eq!(g.extra.len(), 1);
    assert_eq!(g.package_overrides.len(), 1);
}

/// SetPackageLocationRadius and the other package setters keep one changed copy of a
/// package: a call that changes nothing makes no new one (#3150; each copy lives for the
/// session).
#[test]
fn package_setters_called_again_with_the_same_values_make_no_new_copy() {
    use crate::package::{Location, LocationKind, PackageDef, Target, TargetKind};
    use fnv_script::vm::XVal;
    let (forms, refs, mut data) = fixture();
    let p = FormId(0x840);
    data.package_defs.insert(p, PackageDef { target: Some(Target { kind: TargetKind::LinkedReference, count_distance: 3 }), ..Default::default() });
    let mut g = Game::new(&data, World::new(&forms, &refs, Default::default()), Default::default());
    let other = |g: &mut Game<'_>, name: &str, args: &[XVal]| crate::nvse_other::call(g, &Default::default(), name, None, args);
    let rest = |g: &mut Game<'_>, name: &str, args: &[XVal]| crate::nvse_rest::call(g, &Default::default(), name, None, args);
    // A value the record already has: no copy at all.
    assert!(rest(&mut g, "setpackagetargetdistance", &[XVal::Ref(p.0), XVal::Num(3.0)]).is_some());
    assert!(g.package_overrides.is_empty());
    assert!(other(&mut g, "setpackagelocationradius", &[XVal::Ref(p.0), XVal::Num(512.0)]).is_some());
    let first: *const PackageDef = g.package_def(&p).unwrap();
    for _ in 0..1000 {
        other(&mut g, "setpackagelocationradius", &[XVal::Ref(p.0), XVal::Num(512.0)]);
        rest(&mut g, "setpackagetargetdistance", &[XVal::Ref(p.0), XVal::Num(3.0)]);
    }
    assert_eq!(g.package_overrides.len(), 1);
    assert!(std::ptr::eq(g.package_def(&p).unwrap(), first), "the same copy is still the package's definition");
    // A real change still takes: the copy is replaced.
    other(&mut g, "setpackagelocationreference", &[XVal::Ref(p.0), XVal::Ref(0x14)]);
    let def = g.package_def(&p).unwrap();
    assert_eq!(def.location, Some(Location { kind: LocationKind::NearReference(Some(FormId(0x14))), radius: 512 }));
    assert_eq!(def.target.map(|t| t.count_distance), Some(3));
    assert_eq!(g.package_overrides.len(), 1);
}

/// SetPackageFlag writes through `Game::change_package` (so every `Game::package_def`
/// consumer sees it and repeated no-ops keep one allocation per #3150), and authored
/// package overrides survive same-run save loads via `Game::Session` while resetting on a
/// fresh-run restore (and synthetic TravelToRef packages are never carried across loads).
#[test]
fn set_package_flag_uses_package_overrides_and_survives_same_run_session_only() {
    use crate::package::{flags, PackageDef, Target, TargetKind};
    use fnv_script::vm::XVal;
    let (forms, refs, mut data) = fixture();
    let p = FormId(0x840);
    data.package_defs.insert(p, PackageDef { target: Some(Target { kind: TargetKind::LinkedReference, count_distance: 3 }), ..Default::default() });
    let mut g = Game::new(&data, World::new(&forms, &refs, Default::default()), Default::default());
    let gaps = |g: &mut Game<'_>, name: &str, args: &[XVal]| crate::nvse_gaps::call(g, &Default::default(), name, None, args);
    let other = |g: &mut Game<'_>, name: &str, args: &[XVal]| crate::nvse_other::call(g, &Default::default(), name, None, args);

    // No-op SetPackageFlag (clearing an already-clear bit) creates no override copy.
    assert_eq!(gaps(&mut g, "getpackageflag", &[XVal::Ref(p.0), XVal::Num(13.0)]), Some(Ok(XVal::Num(0.0))));
    assert_eq!(gaps(&mut g, "setpackageflag", &[XVal::Ref(p.0), XVal::Num(13.0), XVal::Num(0.0)]), Some(Ok(XVal::Num(0.0))));
    assert!(g.package_overrides.is_empty());

    // Setting ALWAYS_RUN (bit 13) + location radius updates Game::package_def; repeated calls do not reallocate.
    assert_eq!(gaps(&mut g, "setpackageflag", &[XVal::Ref(p.0), XVal::Num(13.0), XVal::Num(1.0)]), Some(Ok(XVal::Num(0.0))));
    assert!(other(&mut g, "setpackagelocationradius", &[XVal::Ref(p.0), XVal::Num(512.0)]).is_some());
    assert_eq!(gaps(&mut g, "getpackageflag", &[XVal::Ref(p.0), XVal::Num(13.0)]), Some(Ok(XVal::Num(1.0))));
    assert!(g.package_def(&p).unwrap().has(flags::ALWAYS_RUN));
    let ptr: *const PackageDef = g.package_def(&p).unwrap();
    for _ in 0..1000 {
        gaps(&mut g, "setpackageflag", &[XVal::Ref(p.0), XVal::Num(13.0), XVal::Num(1.0)]);
    }
    assert!(std::ptr::eq(g.package_def(&p).unwrap(), ptr));

    // A synthetic TravelToRef package is readable via GetPackageFlag, but must not be carried in Game::Session.
    assert_eq!(
        crate::nvse_jip13::call(&mut g, &Default::default(), "traveltoref", Some(FormId::PLAYER_REF), &[XVal::Ref(1), XVal::Num(1.0)]),
        Some(Ok(XVal::Num(0.0)))
    );
    let made = g.extra[&FormId::PLAYER_REF].script_package.unwrap();
    assert_eq!(gaps(&mut g, "getpackageflag", &[XVal::Ref(made.0), XVal::Num(13.0)]), Some(Ok(XVal::Num(1.0))));

    // Fresh-run restore without put_session resets authored overrides and drops synthetic packages.
    let snap = g.snapshot();
    let mut fresh = Game::restore(&data, World::restore(&forms, &refs, &snap.sim).unwrap(), Default::default(), &snap).unwrap();
    assert_eq!(gaps(&mut fresh, "getpackageflag", &[XVal::Ref(p.0), XVal::Num(13.0)]), Some(Ok(XVal::Num(0.0))));
    assert!(!fresh.package_def(&p).unwrap().has(flags::ALWAYS_RUN));
    assert_eq!(fresh.package_def(&p).unwrap().location, None);
    assert!(fresh.package_def(&made).is_none());

    // Same-run save load transfers authored overrides through Game::Session, excluding synthetic packages.
    let session = g.take_session();
    let mut same_run = Game::restore(&data, World::restore(&forms, &refs, &snap.sim).unwrap(), Default::default(), &snap).unwrap();
    same_run.put_session(session);
    assert_eq!(gaps(&mut same_run, "getpackageflag", &[XVal::Ref(p.0), XVal::Num(13.0)]), Some(Ok(XVal::Num(1.0))));
    assert!(same_run.package_def(&p).unwrap().has(flags::ALWAYS_RUN));
    assert_eq!(same_run.package_def(&p).unwrap().location.map(|l| l.radius), Some(512));
    assert!(same_run.package_def(&made).is_none());
}

#[test]
fn temp_ref_is_bounded_strictly_below_bullet_snap_sound_base() {
    const {
        assert!(COMPILED_BASE < TEMP_REF_BASE);
        assert!(TEMP_REF_BASE < BULLET_SNAP_SOUND_BASE);
        assert!(BULLET_SNAP_SOUND_BASE < CAMERA_OVERLAY_BASE);
        assert!(CAMERA_OVERLAY_BASE < LAMBDA_BASE);
        assert!(BULLET_SNAP_SOUND_BASE == 0xFE7E_0000);
        assert!(CAMERA_OVERLAY_BASE == 0xFE7F_0000);
    }

    let (forms, refs, data) = fixture();
    let mut g = Game::new(&data, World::new(&forms, &refs, Default::default()), Default::default());
    let max = (BULLET_SNAP_SOUND_BASE - TEMP_REF_BASE) as usize;
    // Pre-fill up to one slot below BULLET_SNAP_SOUND_BASE so the boundary is exercised directly.
    g.temp_refs.resize(max - 1, (FormId(0), FormId(0)));
    let container = FormId(0x14);
    let item = FormId(0x0001_0001);
    let last_valid = g.temp_ref(container, item);
    assert_eq!(last_valid, FormId(BULLET_SNAP_SOUND_BASE - 1));
    assert_eq!(g.temp_item(last_valid), Some((container, item)));
    // Re-querying an already allocated pair at capacity reuses its existing slot.
    assert_eq!(g.temp_ref(container, item), last_valid);
    // BulletSnap's 0xFE7E_0000..0xFE7F_0000 window and CAMERA_OVERLAY_BASE are never read as temp_item.
    for reserved in [
        FormId(BULLET_SNAP_SOUND_BASE),
        FormId(BULLET_SNAP_SOUND_BASE + 9),
        FormId(CAMERA_OVERLAY_BASE - 1),
        FormId(CAMERA_OVERLAY_BASE),
        FormId(CAMERA_OVERLAY_BASE + 1),
    ] {
        assert_eq!(g.temp_item(reserved), None, "reserved id {reserved:?} must not map to temp_item");
    }
}

#[test]
#[should_panic(expected = "temporary inventory references exhausted")]
fn temp_ref_refuses_allocation_into_bullet_snap_sound_range() {
    let (forms, refs, data) = fixture();
    let mut g = Game::new(&data, World::new(&forms, &refs, Default::default()), Default::default());
    let max = (BULLET_SNAP_SOUND_BASE - TEMP_REF_BASE) as usize;
    g.temp_refs.resize(max, (FormId(0), FormId(0)));
    let _ = g.temp_ref(FormId(0x14), FormId(0x0001_0002));
}

fn fixture() -> (fnv_sim::FormStore, fnv_sim::RefTable, GameData) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let path = root.join("script/functions.json");
    let table = fnv_script::FunctionTable::load(&path).unwrap();
    let json = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let lo = fnv_esm::LoadOrder::new(&[]).unwrap();
    let schema = fnv_esm::Schema::load_dir(root.join("fnv")).unwrap();
    let forms = fnv_sim::FormStore::default();
    let refs = fnv_sim::RefTable::default();
    let data = GameData::build(&lo, &schema, table, &json, &refs);
    (forms, refs, data)
}
