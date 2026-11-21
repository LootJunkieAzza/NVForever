//! Real PhysWorld/script caster controls, without a window or authored hit stand-in.
use super::*;
use fnv_collide::{CollisionObject, Iso, Motion, Shape};
use fnv_esm::raw::{write_subrecords, Subrecord};
use fnv_game::game::{GameEnv, RunCtx};
use fnv_script::vm::XVal;

fn sub(sig: [u8; 4], data: Vec<u8>) -> Subrecord {
    Subrecord {
        sig,
        data,
        extended: false,
    }
}
fn record(sig: [u8; 4], id: u32, subs: &[Subrecord]) -> Vec<u8> {
    let body = write_subrecords(subs);
    [
        sig.to_vec(),
        (body.len() as u32).to_le_bytes().to_vec(),
        0u32.to_le_bytes().to_vec(),
        id.to_le_bytes().to_vec(),
        vec![0; 4],
        15u16.to_le_bytes().to_vec(),
        vec![0; 2],
        body,
    ]
    .concat()
}
fn state() -> ScriptState {
    let npc = record(
        *b"NPC_",
        7,
        &[
            sub(*b"EDID", b"Player\0".to_vec()),
            sub(*b"ACBS", vec![0; 24]),
        ],
    );
    let group = [
        b"GRUP".to_vec(),
        ((24 + npc.len()) as u32).to_le_bytes().to_vec(),
        b"NPC_".to_vec(),
        vec![0; 12],
        npc,
    ]
    .concat();
    let header = record(
        *b"TES4",
        0,
        &[sub(
            *b"HEDR",
            [
                1.34f32.to_le_bytes(),
                0u32.to_le_bytes(),
                0x1000u32.to_le_bytes(),
            ]
            .concat(),
        )],
    );
    let plugins =
        vec![
            fnv_esm::Plugin::parse(&[header, group].concat(), "OwnedPhysicalLean.esm".into())
                .unwrap(),
        ];
    let mut s = ScriptState::over_plugins(&plugins);
    let g = &mut s.rt.as_mut().unwrap().game;
    g.cfg.strict = true;
    let p = g.world.get(FormId::PLAYER_REF).unwrap();
    p.pos = [0.0; 3];
    p.rot = [0.0; 3];
    s
}
fn play() -> PlayData {
    PlayData {
        lo: Box::leak(Box::new(fnv_esm::LoadOrder::new(&[]).unwrap())),
        schema: Box::leak(Box::default()),
        index: Box::leak(Box::default()),
        world: 0,
        interior: None,
        eye: [0.0; 3],
        heading_deg: 0.0,
        pitch_deg: 0.0,
        auto_walk: 0.0,
        start_at_cell_spot: false,
        packages: false,
    }
}
fn object(half: [f32; 3], motion: Motion) -> CollisionObject {
    CollisionObject {
        node: None,
        node_block: 0,
        node_transform: Iso::IDENTITY,
        body_transform: Iso::IDENTITY,
        motion,
        layer: fnv_phys::layers::STATIC,
        mass: 20.0,
        friction: 0.5,
        restitution: 0.0,
        shape: Shape::Box { half_extents: half }.into(),
        shell: 0.0,
        material: None,
        wind: false,
    }
}
fn wall(p: &mut PhysState, id: u32, pos: [f32; 3], half: [f32; 3], motion: Motion) {
    let report = p.world.add_ref(
        &fnv_phys::RefPose {
            form_id: id,
            position: pos,
            rotation: Iso::IDENTITY.rotation,
            scale: 1.0,
        },
        &[object(half, motion)],
    );
    assert!(
        report.errors.is_empty(),
        "real collision fixture must be admitted: {:?}",
        report.errors
    );
}
fn query(s: &mut ScriptState, body: &str) -> f64 {
    let g = &mut s.rt.as_mut().unwrap().game;
    let source = format!("Begin Function {{}}\n{body}\nEnd\n");
    let c = fnv_script::compile::compile_runtime(&source, &g.data.table, &GameEnv { g: &*g });
    assert!(
        c.errors().next().is_none(),
        "physical-clearance-compile: {:?}",
        c.diags
    );
    match g
        .run_query(&source, RunCtx::default())
        .unwrap_or_else(|e| panic!("physical-clearance-source: {e}"))
    {
        XVal::Num(n) => n,
        x => panic!("clearance must be a scalar distance: {x:?}"),
    }
}
#[test]
fn ordinary_source_queries_measure_asymmetric_static_walls_and_preserve_other_ray_filters() {
    let (mut phys, mut script, data) = (PhysState::default(), state(), play());
    wall(
        &mut phys,
        0xE80,
        [31.0, 0.0, 64.0],
        [1.0, 100.0, 100.0],
        Motion::Static,
    );
    wall(
        &mut phys,
        0xE81,
        [-121.0, 0.0, 64.0],
        [1.0, 100.0, 100.0],
        Motion::Static,
    );
    wall(
        &mut phys,
        0xE82,
        [5.0, 0.0, 64.0],
        [1.0, 10.0, 10.0],
        Motion::Dynamic,
    );
    let _actor = phys.world.add_character(
        0xE84,
        fnv_phys::CharacterParams {
            radius: 2.0,
            height: 128.0,
            bottom_slope: 0.0,
            bottom_flat: 0.0,
            ..Default::default()
        },
        [10.0, 0.0, 0.0],
    );
    phys.world.add_terrain(
        (0, 0),
        &vec![0.0; fnv_world::land::SIDE * fnv_world::land::SIDE],
    );
    with_ray_caster(&mut phys, &data, 0, || {
        let right = query(
            &mut script,
            "SetFunctionValue (Player.GetVATSRightAreaFree)",
        );
        let left = query(&mut script, "SetFunctionValue (Player.GetVATSLeftAreaFree)");
        assert!((right - 30.0).abs() < 1e-3, "physical-right: {right}");
        assert!((left - 120.0).abs() < 1e-3, "physical-left: {left}");
        let existing = fnv_game::ray::cast(
            [0.0, 0.0, 64.0],
            [1024.0, 0.0, 64.0],
            fnv_game::ray::CastKind::World,
        )
        .unwrap();
        assert!(
            (existing - 4.0).abs() < 1e-3,
            "ordinary World rays must still meet dynamic clutter: {existing}"
        );
        let ground = fnv_game::ray::cast(
            [512.0, 512.0, 64.0],
            [512.0, 512.0, -64.0],
            fnv_game::ray::CastKind::CameraClearance,
        )
        .unwrap();
        assert!(
            (ground - 64.0).abs() < 1e-3,
            "static-camera filtering must retain actual terrain: {ground}"
        );
    });
    assert!(phys.world.remove_ref(0xE82));
    with_ray_caster(&mut phys, &data, 0, || {
        let ordinary = fnv_game::ray::cast(
            [0.0, 0.0, 64.0],
            [1024.0, 0.0, 64.0],
            fnv_game::ray::CastKind::World,
        )
        .unwrap();
        assert!(
            (ordinary - 8.0).abs() < 1e-3,
            "ordinary ray must actually meet the authored character: {ordinary}"
        );
        let clearance = query(
            &mut script,
            "SetFunctionValue (Player.GetVATSRightAreaFree)",
        );
        assert!(
            (clearance - 30.0).abs() < 1e-3,
            "camera-clearance must exclude that actual actor: {clearance}"
        );
    });
}
#[test]
fn current_heading_and_scene_eviction_are_measured_without_retaining_caster_state() {
    let (mut phys, mut script, data) = (PhysState::default(), state(), play());
    wall(
        &mut phys,
        0xE80,
        [31.0, 0.0, 64.0],
        [1.0, 100.0, 100.0],
        Motion::Static,
    );
    with_ray_caster(&mut phys, &data, 0, || {
        assert!(
            (query(
                &mut script,
                "SetFunctionValue (Player.GetVATSRightAreaFree)"
            ) - 30.0)
                .abs()
                < 1e-3
        );
    });
    assert!(phys.world.remove_ref(0xE80));
    with_ray_caster(&mut phys, &data, 0, || {
        assert_eq!(
            query(
                &mut script,
                "SetFunctionValue (Player.GetVATSRightAreaFree)"
            ),
            1024.0,
            "scene removal must remove the actual obstruction"
        );
    });
    wall(
        &mut phys,
        0xE83,
        [0.0, -51.0, 64.0],
        [100.0, 1.0, 100.0],
        Motion::Static,
    );
    with_ray_caster(&mut phys, &data, 0, || {
        let measured = query(
            &mut script,
            "Player.SetAngle Z 90\nSetFunctionValue (Player.GetVATSRightAreaFree)",
        );
        assert!(
            (measured - 50.0).abs() < 1e-3,
            "current-heading: {measured}"
        );
        assert_eq!(
            query(&mut script, "SetFunctionValue (Player.GetVATSLeftAreaFree)"),
            1024.0
        );
    });
    assert_eq!(
        query(
            &mut script,
            "SetFunctionValue (Player.GetVATSRightAreaFree)"
        ),
        0.0,
        "query context must not keep a previous physical world"
    );
}
