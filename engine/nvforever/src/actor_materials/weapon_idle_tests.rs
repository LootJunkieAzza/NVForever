use std::sync::Arc;

use bevy::math::Affine2;
use bevy::prelude::*;
use fnv_nif::anim::{Channel, Key, KeyKind, Keys, ValueTrack};
use nvf_assets::actor::{PreparedSkeleton, ShapeName};
use nvf_assets::anim_play::{Clip, MaterialTarget, MaterialTrack, OwnCycle};
use nvf_assets::material_anim::{Look, StoredLook};
use nvf_assets::NifMaterial;

use super::{append_weapon_material_values, pose_player, PlayerRigMaterials};
use crate::limbs::ActorRig;
use crate::player_body::PlayerBody;

fn linear_keys(pts: &[(f32, f32)]) -> Keys<1> {
    Keys {
        kind: KeyKind::Linear,
        keys: pts
            .iter()
            .map(|&(time, v)| Key { time, value: [v], forward: [0.0], backward: [0.0], tbc: [0.0; 3] })
            .collect(),
    }
}

fn shishkebab_flame_clip() -> Arc<Clip> {
    let uv_track = |axis: usize, stop: f32| MaterialTrack {
        shape: "FlameEffectMesh".into(),
        target: MaterialTarget::UvOffset(axis),
        values: Arc::new(ValueTrack::Float(Channel::Keys(linear_keys(&[(0.0, 0.0), (stop, 1.0)])))),
        cycle: Some(OwnCycle { start: 0.0, stop, frequency: 1.0, phase: 0.0, cycle_type: 0 }),
        priority: 0,
    };
    Arc::new(Clip::from_controllers(vec![uv_track(1, 1.0), uv_track(0, 4.0)], Vec::new(), Vec::new()))
}

#[test]
fn held_weapon_idle_material_tracks_scroll_in_first_and_third_person() {
    let mut app = App::new();
    app.init_resource::<Assets<NifMaterial>>()
        .init_resource::<PlayerRigMaterials>()
        .add_systems(Update, pose_player);

    let shared = app.world_mut().resource_mut::<Assets<NifMaterial>>().add(NifMaterial::default());
    let skel = Arc::new(PreparedSkeleton::default());

    let spawn_rig = |world: &mut World| {
        let root = world.spawn_empty().id();
        let flame = world
            .spawn((
                MeshMaterial3d(shared.clone()),
                ShapeName("flameeffectmesh".into()),
                StoredLook(Look::stored([1.0; 3], 1.0, 1.0)),
                ChildOf(root),
            ))
            .id();
        let actor = ActorRig {
            reference: 0,
            base: 0,
            body: 0,
            skeleton: String::new(),
            skel: skel.clone(),
            nodes: vec![root],
            base_scale: 1.0,
        };
        (root, actor, flame)
    };

    let (third_root, third_actor, third_flame) = spawn_rig(app.world_mut());
    let (first_root, first_actor, first_flame) = spawn_rig(app.world_mut());

    let clip = shishkebab_flame_clip();
    let mut pb = PlayerBody::synthetic_script_models((third_root, third_actor), Some((first_root, first_actor)));
    pb.set_weapon_materials(false, Some((clip.clone(), 0.25)));
    pb.set_weapon_materials(true, Some((clip.clone(), 0.25)));
    app.world_mut().insert_resource(pb);

    app.update();

    let uv_of = |app: &App, entity: Entity| {
        let handle = &app.world().get::<MeshMaterial3d<NifMaterial>>(entity).unwrap().0;
        assert_ne!(handle, &shared, "driven weapon shape must receive a private material copy");
        app.world().resource::<Assets<NifMaterial>>().get(handle).unwrap().base.uv_transform
    };

    let expected_first = Affine2::from_translation(Vec2::new(0.0625, 0.25));
    assert!(uv_of(&app, third_flame).abs_diff_eq(expected_first, 1e-5));
    assert!(uv_of(&app, first_flame).abs_diff_eq(expected_first, 1e-5));
    assert!(
        app.world().resource::<Assets<NifMaterial>>().get(&shared).unwrap().base.uv_transform.abs_diff_eq(Affine2::IDENTITY, 1e-6),
        "shared weapon material asset must stay untouched"
    );

    // Advancing the weapon clock past V's 1.0 s cycle wraps V while U continues smoothly.
    app.world_mut().resource_mut::<PlayerBody>().set_weapon_materials(false, Some((clip.clone(), 1.5)));
    app.world_mut().resource_mut::<PlayerBody>().set_weapon_materials(true, Some((clip, 1.5)));
    app.update();

    let expected_second = Affine2::from_translation(Vec2::new(0.375, 0.5));
    assert!(uv_of(&app, third_flame).abs_diff_eq(expected_second, 1e-5));
    assert!(uv_of(&app, first_flame).abs_diff_eq(expected_second, 1e-5));

    // Clearing weapon materials (unequip/swap) restores the stored identity UV transform.
    app.world_mut().resource_mut::<PlayerBody>().set_weapon_materials(false, None);
    app.world_mut().resource_mut::<PlayerBody>().set_weapon_materials(true, None);
    app.update();

    assert!(uv_of(&app, third_flame).abs_diff_eq(Affine2::IDENTITY, 1e-5));
    assert!(uv_of(&app, first_flame).abs_diff_eq(Affine2::IDENTITY, 1e-5));
}

#[test]
fn actor_clip_material_track_takes_precedence_over_weapon_idle() {
    let clip = shishkebab_flame_clip();
    let mut values = vec![("FlameEffectMesh", MaterialTarget::UvOffset(1), [0.9, 0.0, 0.0], 1.0)];
    append_weapon_material_values(&clip, 0.25, &mut values);

    assert_eq!(values.len(), 2, "only undriven U axis should be appended");
    let v = values.iter().find(|(_, t, ..)| *t == MaterialTarget::UvOffset(1)).unwrap();
    let u = values.iter().find(|(_, t, ..)| *t == MaterialTarget::UvOffset(0)).unwrap();
    assert!((v.2[0] - 0.9).abs() < 1e-6, "actor clip track must win on V");
    assert!((u.2[0] - 0.0625).abs() < 1e-6, "weapon idle track still drives U");
}
