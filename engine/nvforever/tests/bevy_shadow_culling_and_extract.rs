//! Headless tests for vendored Bevy directional shadow culling (`ortho_xy` + `radius_sq`) (#4720).
use bevy::camera::primitives::{Aabb, CascadesFrusta, Frustum};
use bevy::camera::visibility::{
    CascadesVisibleEntities, InheritedVisibility, NoFrustumCulling, SetViewVisibility,
    ViewVisibility,
};
use bevy::camera::{CameraProjection, OrthographicProjection, Projection};
use bevy::light::cascade::{Cascade, Cascades};
use bevy::light::{check_dir_light_mesh_visibility, DirectionalLight};
use bevy::math::{Mat4, Quat};
use bevy::prelude::*;

/// Build an axis-aligned orthographic cascade `[-extent, extent]` in X/Y and `[-100, 100]` in Z.
fn ortho_cascade(extent: f32, texel_size: f32) -> (Cascade, Frustum) {
    let ortho = OrthographicProjection {
        area: Rect::from_center_half_size(Vec2::ZERO, Vec2::splat(extent)),
        near: -100.0,
        far: 100.0,
        ..OrthographicProjection::default_3d()
    };
    let proj = ortho.get_clip_from_view();
    let frustum = Projection::Orthographic(ortho).compute_frustum(&GlobalTransform::IDENTITY);
    let cascade = Cascade {
        world_from_cascade: Mat4::IDENTITY,
        clip_from_cascade: proj,
        clip_from_world: proj,
        texel_size,
    };
    (cascade, frustum)
}

#[test]
fn dir_light_culling_ortho_xy_and_radius_sq_preserve_exact_visibility() {
    let mut app = App::new();
    app.add_systems(Update, check_dir_light_mesh_visibility);

    let cam = app.world_mut().spawn_empty().id();
    let (cascade, frustum) = ortho_cascade(10.0, 1.0);

    let mut frusta = CascadesFrusta::default();
    frusta.frusta.insert(cam, vec![frustum]);
    let mut cascades = Cascades::default();
    cascades.cascades.insert(cam, vec![cascade]);

    let light = app
        .world_mut()
        .spawn((
            DirectionalLight {
                shadow_maps_enabled: true,
                ..default()
            },
            frusta,
            cascades,
            CascadesVisibleEntities::default(),
            ViewVisibility::HIDDEN,
        ))
        .id();
    app.world_mut()
        .get_mut::<ViewVisibility>(light)
        .unwrap()
        .set_visible();

    // 1) Inside cascade and large enough (> 1 texel): MUST be visible.
    let inside = app
        .world_mut()
        .spawn((
            Mesh3d::default(),
            InheritedVisibility::VISIBLE,
            ViewVisibility::HIDDEN,
            Aabb::from_min_max(Vec3::splat(-2.0), Vec3::splat(2.0)),
            GlobalTransform::from(Transform::from_xyz(3.0, -4.0, 0.0)),
        ))
        .id();

    // 2) Far outside in X (rejected by ortho_xy pre-reject): MUST be culled.
    let far_x = app
        .world_mut()
        .spawn((
            Mesh3d::default(),
            InheritedVisibility::VISIBLE,
            ViewVisibility::HIDDEN,
            Aabb::from_min_max(Vec3::splat(-2.0), Vec3::splat(2.0)),
            GlobalTransform::from(Transform::from_xyz(50.0, 0.0, 0.0)),
        ))
        .id();

    // 3) Far outside in Y (rejected by ortho_xy pre-reject): MUST be culled.
    let far_y = app
        .world_mut()
        .spawn((
            Mesh3d::default(),
            InheritedVisibility::VISIBLE,
            ViewVisibility::HIDDEN,
            Aabb::from_min_max(Vec3::splat(-2.0), Vec3::splat(2.0)),
            GlobalTransform::from(Transform::from_xyz(0.0, -50.0, 0.0)),
        ))
        .id();

    // 4) Straddling the X boundary (center at 11.0, half-extent 2.0 -> overlaps [-10, 10]): MUST be visible.
    let straddle = app
        .world_mut()
        .spawn((
            Mesh3d::default(),
            InheritedVisibility::VISIBLE,
            ViewVisibility::HIDDEN,
            Aabb::from_min_max(Vec3::splat(-2.0), Vec3::splat(2.0)),
            GlobalTransform::from(Transform::from_xyz(11.0, 0.0, 0.0)),
        ))
        .id();

    // 5) Sub-texel caster inside cascade (radius = sqrt(3)*0.1 < 1.0 texel): MUST be culled by radius_sq.
    let tiny = app
        .world_mut()
        .spawn((
            Mesh3d::default(),
            InheritedVisibility::VISIBLE,
            ViewVisibility::HIDDEN,
            Aabb::from_min_max(Vec3::splat(-0.1), Vec3::splat(0.1)),
            GlobalTransform::from(Transform::from_xyz(0.0, 0.0, 0.0)),
        ))
        .id();

    // 6) Far outside + tiny, but marked NoFrustumCulling: MUST stay visible.
    let no_cull = app
        .world_mut()
        .spawn((
            Mesh3d::default(),
            InheritedVisibility::VISIBLE,
            ViewVisibility::HIDDEN,
            Aabb::from_min_max(Vec3::splat(-0.05), Vec3::splat(0.05)),
            GlobalTransform::from(Transform::from_xyz(500.0, 500.0, 0.0)),
            NoFrustumCulling,
        ))
        .id();

    app.update();

    let vis = app.world().get::<CascadesVisibleEntities>(light).unwrap();
    let list = &vis.entities.get(&cam).unwrap()[0].entities;
    assert!(list.contains(&inside), "inside caster must be visible");
    assert!(list.contains(&straddle), "boundary-straddling caster must not be falsely rejected by ortho_xy");
    assert!(list.contains(&no_cull), "NoFrustumCulling caster must stay visible");
    assert!(!list.contains(&far_x), "far X caster must be culled");
    assert!(!list.contains(&far_y), "far Y caster must be culled");
    assert!(!list.contains(&tiny), "sub-texel caster must be culled by radius_sq");
}

#[test]
fn dir_light_culling_rotated_scaled_obb_and_non_ortho_fallback() {
    let mut app = App::new();
    app.add_systems(Update, check_dir_light_mesh_visibility);

    let cam = app.world_mut().spawn_empty().id();
    let (ortho_c, frustum) = ortho_cascade(10.0, 1.0);
    // Second cascade has a non-orthographic row 3 so ortho_xy is disabled and Frustum::intersects_obb runs directly.
    let mut non_ortho_c = ortho_c.clone();
    non_ortho_c.clip_from_world.w_axis = Vec4::new(0.0, 0.0, -1.0, 1.0);
    non_ortho_c.clip_from_world.z_axis.w = -1.0;

    let mut frusta = CascadesFrusta::default();
    frusta.frusta.insert(cam, vec![frustum, frustum]);
    let mut cascades = Cascades::default();
    cascades.cascades.insert(cam, vec![ortho_c, non_ortho_c]);

    let light = app
        .world_mut()
        .spawn((
            DirectionalLight {
                shadow_maps_enabled: true,
                ..default()
            },
            frusta,
            cascades,
            CascadesVisibleEntities::default(),
            ViewVisibility::HIDDEN,
        ))
        .id();
    app.world_mut()
        .get_mut::<ViewVisibility>(light)
        .unwrap()
        .set_visible();

    // Non-uniformly scaled & rotated OBB whose corner reaches inside x = 10.0 from center x = 12.0.
    let rotated_straddle = app
        .world_mut()
        .spawn((
            Mesh3d::default(),
            InheritedVisibility::VISIBLE,
            ViewVisibility::HIDDEN,
            Aabb::from_min_max(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0)),
            GlobalTransform::from(
                Transform::from_xyz(12.0, 0.0, 0.0)
                    .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_4))
                    .with_scale(Vec3::new(3.0, 1.0, 1.0)),
            ),
        ))
        .id();

    let far_outside = app
        .world_mut()
        .spawn((
            Mesh3d::default(),
            InheritedVisibility::VISIBLE,
            ViewVisibility::HIDDEN,
            Aabb::from_min_max(Vec3::splat(-1.0), Vec3::splat(1.0)),
            GlobalTransform::from(Transform::from_xyz(-40.0, 0.0, 0.0)),
        ))
        .id();

    app.update();

    let vis = app.world().get::<CascadesVisibleEntities>(light).unwrap();
    let lists = vis.entities.get(&cam).unwrap();
    assert!(lists[0].entities.contains(&rotated_straddle), "rotated/scaled straddling OBB must stay visible in ortho cascade");
    assert!(!lists[0].entities.contains(&far_outside), "far caster must be culled in ortho cascade");
    assert!(lists[1].entities.contains(&rotated_straddle), "non-ortho fallback must keep straddling OBB visible");
    assert!(!lists[1].entities.contains(&far_outside), "non-ortho fallback must still cull far caster via frustum");
}
