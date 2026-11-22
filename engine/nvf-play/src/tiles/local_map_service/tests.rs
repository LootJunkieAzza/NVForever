//! Comprehensive CPU/ECS acceptance tests for Issue #2418 (local-map images for
//! registered mod UI recipients).

use super::*;
use crate::tiles::draw::{self, TestPainted, TileFonts};
use crate::tiles::runtime_images::RuntimeImages;
use fnv_ui::{MemSource, Ui};

fn standard_fields() -> FieldBinding {
    FieldBinding {
        active_trait: Some("_LocalMode".into()),
        update_trait: Some("_ForceUpdate".into()),
        window_width_trait: "_WinW".into(),
        window_height_trait: "_WinH".into(),
        scale_trait: "_Scale".into(),
        center_x_trait: None,
        center_y_trait: None,
        basis_angle_trait: Some("_BasisAngle".into()),
        basis_cos_trait: Some("_AxisCos".into()),
        basis_sin_trait: Some("_AxisSin".into()),
        origin_x_sink: "_OriginX".into(),
        origin_y_sink: "_OriginY".into(),
        span_x_sink: Some("_SpanX".into()),
        span_y_sink: Some("_SpanY".into()),
        filename_sink: "filename".into(),
    }
}

fn menu_xml(extra: &str) -> String {
    format!(
        r#"<menu name="OwnedHud">
  <template name="MapLeafTpl">
    <image name="MapLeaf">
      <filename>Interface\Shared\solid.dds</filename>
      <width>64</width>
      <height>48</height>
      <zoom>-1</zoom>
      <visible>1</visible>
      <rotateaxisx>32</rotateaxisx>
      <rotateaxisy>24</rotateaxisy>
      <rotateangle>0.5</rotateangle>
    </image>
  </template>
  <rect name="Panel">
    <_LocalMode>1</_LocalMode>
    <_ForceUpdate>0</_ForceUpdate>
    <_WinW>40</_WinW>
    <_WinH>30</_WinH>
    <_Scale>8</_Scale>
    <_BasisAngle>0</_BasisAngle>
    <_AxisCos>1</_AxisCos>
    <_AxisSin>0</_AxisSin>
    <_OriginX>-999999</_OriginX>
    <_OriginY>-999999</_OriginY>
    <_SpanX>0</_SpanX>
    <_SpanY>0</_SpanY>
    <_MarkerX><copy src="me()" trait="_OriginX" /><add>100</add></_MarkerX>
    <image name="MapLeaf">
      <filename>Interface\Shared\solid.dds</filename>
      <width>64</width>
      <height>48</height>
      <zoom>-1</zoom>
      <visible>1</visible>
      <rotateaxisx>32</rotateaxisx>
      <rotateaxisy>24</rotateaxisy>
      <rotateangle>0.5</rotateangle>
    </image>
    <image name="SiblingPlaceholder">
      <filename>Interface\Shared\solid.dds</filename>
      <width>64</width>
      <height>48</height>
      <visible>1</visible>
    </image>
  </rect>
  {extra}
</menu>"#
    )
}

fn load_ui(xml: &str) -> (Ui, TileId) {
    let src = MemSource::default().with("owned_hud.xml", xml);
    let mut ui = Ui::new();
    let root = ui.load_menu(&src, "owned_hud.xml").unwrap();
    ui.update();
    (ui, root)
}

fn panel_config(id: &str) -> RecipientConfig {
    RecipientConfig {
        id: RecipientId::new(id).unwrap(),
        source_path: "OwnedHud/Panel".into(),
        recipient_path: "OwnedHud/Panel/MapLeaf".into(),
        fields: standard_fields(),
    }
}

/// Owned asymmetric synthetic geometry provider:
/// - Interior cell 1: an L-shaped room in the north-west quadrant (x < 0, y > 0) with a
///   raised counter on floor 0, and a completely different south-east room on floor 2 (feet >= 256).
/// - Interior cell 2: a narrow eastern corridor (x > 40).
/// - Exterior world 7: a western cliff wall at x > 200.
struct SyntheticProvider {
    state: ProviderState,
    rays: usize,
}

impl SyntheticProvider {
    fn interior(cell: u32, feet: [f32; 3], static_rev: u64) -> Self {
        Self {
            state: ProviderState::Ready(ProviderSnapshot { interior: Some(cell), world: 0, feet, static_rev }),
            rays: 0,
        }
    }

    fn exterior(world: u32, feet: [f32; 3], static_rev: u64) -> Self {
        Self {
            state: ProviderState::Ready(ProviderSnapshot { interior: None, world, feet, static_rev }),
            rays: 0,
        }
    }
}

impl LocalMapProvider for SyntheticProvider {
    fn state(&self) -> ProviderState {
        self.state
    }

    fn sample_ray(&mut self, origin: [f32; 3], _reach: f32) -> Option<f32> {
        self.rays += 1;
        let ProviderState::Ready(snap) = self.state else { return None };
        match snap.interior {
            Some(1) => {
                if snap.feet[2] >= 200.0 {
                    // Upper floor: room strictly in south-east (x > 0, y < 0).
                    (origin[0] >= 10.0 && origin[0] <= 180.0 && origin[1] <= -10.0 && origin[1] >= -140.0)
                        .then_some(snap.feet[2])
                } else {
                    // Ground floor: asymmetric L-shape strictly in north-west (x <= 0, y >= 0).
                    let in_arm_a = origin[0] >= -180.0 && origin[0] <= -10.0 && origin[1] >= 10.0 && origin[1] <= 150.0;
                    let in_arm_b = origin[0] >= -180.0 && origin[0] <= -80.0 && origin[1] >= -90.0 && origin[1] <= 150.0;
                    let counter = origin[0] >= -130.0 && origin[0] <= -90.0 && origin[1] >= 40.0 && origin[1] <= 90.0;
                    (in_arm_a || in_arm_b).then_some(if counter { 80.0 } else { 0.0 })
                }
            }
            Some(2) => (origin[0] >= 40.0 && origin[0] <= 160.0 && origin[1].abs() <= 80.0).then_some(snap.feet[2]),
            Some(_) => None,
            None => Some(if origin[0] > 200.0 { 500.0 } else { 0.0 }),
        }
    }
}

/// Count non-zero alpha pixels in the 4 quadrants: (NW, NE, SW, SE).
fn quadrant_counts(img: &Image) -> (usize, usize, usize, usize) {
    let w = img.texture_descriptor.size.width as usize;
    let h = img.texture_descriptor.size.height as usize;
    let data = img.data.as_deref().expect("image RGBA data");
    let (mut nw, mut ne, mut sw, mut se) = (0, 0, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if data[(y * w + x) * 4 + 3] > 0 {
                match (x < w / 2, y < h / 2) {
                    (true, true) => nw += 1,
                    (false, true) => ne += 1,
                    (true, false) => sw += 1,
                    (false, false) => se += 1,
                }
            }
        }
    }
    (nw, ne, sw, se)
}

#[test]
fn asymmetric_geometry_proves_origin_scale_basis_and_transitions_across_floor_cell_world_zoom_and_cancel() {
    let (mut ui, _) = load_ui(&menu_xml(""));
    let mut service = LocalMapService::default();
    let id = RecipientId::new("main_map").unwrap();
    service.register(panel_config("main_map")).unwrap();

    let mut runtime = RuntimeImages::default();
    let mut images = Assets::<Image>::default();
    let epoch = SessionEpoch { load_epoch: 0, tree: ui.screen() };

    // 1. Interior cell 1, ground floor (feet z=0): NW-heavy asymmetric L-room.
    let mut provider = SyntheticProvider::interior(1, [0.0, 0.0, 0.0], 1);
    for _ in 0..4 {
        service.tick(epoch, &mut ui, &mut provider, &mut runtime, &mut images);
    }
    ui.update();

    let (img_name_1, handle_1, rev_1, origin_1, span_1, size_1) = match service.diagnostic(&id) {
        RecipientDiagnostic::Bound { image_name, revision, origin, span, size, .. } => {
            let h = runtime.get(&image_name).unwrap().image.clone();
            (image_name, h, revision, origin, span, size)
        }
        other => panic!("expected Bound, got {other:?}"),
    };
    assert_eq!(ui.get_float("OwnedHud/Panel/_OriginX"), Some(f64::from(origin_1[0])));
    assert_eq!(ui.get_float("OwnedHud/Panel/_OriginY"), Some(f64::from(origin_1[1])));
    assert_eq!(ui.get_float("OwnedHud/Panel/_SpanX"), Some(f64::from(span_1.x)));
    assert_eq!(ui.get_float("OwnedHud/Panel/_SpanY"), Some(f64::from(span_1.y)));
    assert_eq!(ui.get_float("OwnedHud/Panel/_MarkerX"), Some(f64::from(origin_1[0]) + 100.0), "dependent marker expression tracks published origin");
    assert_eq!(ui.get_string("OwnedHud/Panel/MapLeaf/filename").as_deref(), Some(img_name_1.as_str()));

    let (nw, ne, _sw, se) = quadrant_counts(images.get(&handle_1).unwrap());
    assert!(nw > 0 && ne == 0 && se == 0, "WorldNorth NW L-room must populate NW quadrant only: nw={nw} ne={ne} se={se}");

    // 2. Floor change (feet z=300 crosses FEET_SNAP): SE-only room on upper floor; stable handle reused, revision advances.
    let mut upper = SyntheticProvider::interior(1, [0.0, 0.0, 300.0], 1);
    service.tick(epoch, &mut ui, &mut upper, &mut runtime, &mut images);
    let rev_2 = match service.diagnostic(&id) {
        RecipientDiagnostic::Bound { image_name, revision, .. } => {
            assert_eq!(image_name, img_name_1, "stable runtime image name preserved");
            assert_eq!(runtime.get(&image_name).unwrap().image, handle_1, "stable Handle<Image> reused in place");
            revision
        }
        other => panic!("expected Bound on upper floor, got {other:?}"),
    };
    assert!(rev_2 > rev_1);
    let (nw2, ne2, sw2, se2) = quadrant_counts(images.get(&handle_1).unwrap());
    assert!(se2 > 0 && nw2 == 0 && ne2 == 0 && sw2 == 0, "upper floor SE room must populate SE quadrant only: ({nw2},{ne2},{sw2},{se2})");

    // 3. Cell change (interior 2): eastern corridor shifts lit pixels to east half.
    let mut cell2 = SyntheticProvider::interior(2, [0.0, 0.0, 0.0], 2);
    service.tick(epoch, &mut ui, &mut cell2, &mut runtime, &mut images);
    let (nw3, ne3, sw3, se3) = quadrant_counts(images.get(&handle_1).unwrap());
    assert!(ne3 + se3 > 0 && nw3 + sw3 == 0, "cell 2 eastern corridor populates east quadrants only: ({nw3},{ne3},{sw3},{se3})");

    // 4. Zoom/scale change (_Scale 8 -> 16): span and dimensions grow cleanly (exceeds 1 frame ray cap,
    //    finishes cleanly on the next frame).
    ui.set_float("OwnedHud/Panel/_Scale", 16.0);
    ui.update();
    service.tick(epoch, &mut ui, &mut cell2, &mut runtime, &mut images);
    assert!(matches!(service.diagnostic(&id), RecipientDiagnostic::Pending { sampled: 512, total, .. } if total > 512));
    service.tick(epoch, &mut ui, &mut cell2, &mut runtime, &mut images);
    match service.diagnostic(&id) {
        RecipientDiagnostic::Bound { span, size, .. } => {
            assert!(span.x > span_1.x && size[0] > size_1[0], "zooming out expands world span and grid dimensions");
        }
        other => panic!("expected Bound after zoom, got {other:?}"),
    }

    // 5. Exterior world change with tight ray budget: starts Pending, then canceling via deactivation
    //    aborts stale work cleanly and restores owned sinks to pre-binding values.
    let mut tight = LocalMapService::with_budget(ServiceBudget { max_rays_per_frame: 16, max_rays_per_batch: 16, ..Default::default() });
    tight.register(panel_config("main_map")).unwrap();
    let (mut ui2, _) = load_ui(&menu_xml(""));
    let epoch2 = SessionEpoch { load_epoch: 0, tree: ui2.screen() };
    let mut ext = SyntheticProvider::exterior(7, [0.0, 0.0, 0.0], 3);
    tight.tick(epoch2, &mut ui2, &mut ext, &mut runtime, &mut images);
    assert!(matches!(tight.diagnostic(&id), RecipientDiagnostic::Pending { sampled: 16, total, .. } if total > 16));

    // Deactivate mid-sampling: cancels pending job without ever publishing stale pixels or origin.
    ui2.set_float("OwnedHud/Panel/_LocalMode", 0.0);
    ui2.update();
    tight.tick(epoch2, &mut ui2, &mut ext, &mut runtime, &mut images);
    assert_eq!(tight.diagnostic(&id), RecipientDiagnostic::Inactive);
    assert_eq!(tight.counters().jobs_canceled, 1);
    assert_eq!(ui2.get_float("OwnedHud/Panel/_OriginX"), Some(-999999.0), "canceled job never writes origin");
    assert_eq!(ui2.get_string("OwnedHud/Panel/MapLeaf/filename").as_deref(), Some(r"Interface\Shared\solid.dds"));

    // 6. Oriented sampling basis (_BasisAngle != 0) is strictly rejected (WorldNorth-only sampling).
    ui.set_float("OwnedHud/Panel/_BasisAngle", 0.75);
    ui.update();
    service.tick(epoch, &mut ui, &mut cell2, &mut runtime, &mut images);
    assert_eq!(
        service.diagnostic(&id),
        RecipientDiagnostic::InvalidRequest(RequestRejection::Grid(GridRejected::UnsupportedOrientation))
    );
}

#[test]
fn real_assets_image_mutation_and_runtime_revision_reach_normal_painter_with_recipient_axes() {
    use crate::script::ScriptState;

    let (ui, menu) = load_ui(&menu_xml(""));
    let recipient_tile = ui.find_tile("OwnedHud/Panel/MapLeaf").unwrap();
    let sibling_tile = ui.find_tile("OwnedHud/Panel/SiblingPlaceholder").unwrap();

    let mut app = App::new();
    app.init_resource::<Assets<Image>>()
        .init_resource::<TileFonts>()
        .init_resource::<crate::pad::PadGlyphs>()
        .init_resource::<RuntimeImages>()
        .insert_resource(ScriptState::over_no_plugins())
        .insert_resource(TestPainted(ui, menu))
        .add_systems(Update, draw::paint_test_menu);

    let mut service = LocalMapService::default();
    let id = RecipientId::new("hud_local").unwrap();
    service.register(panel_config("hud_local")).unwrap();

    // Run service against TestPainted's Ui + App's real Assets<Image> and RuntimeImages.
    let mut provider = SyntheticProvider::interior(1, [0.0, 0.0, 0.0], 1);
    app.world_mut().resource_scope(|world, mut painted: Mut<TestPainted>| {
        world.resource_scope(|world, mut runtime: Mut<RuntimeImages>| {
            world.resource_scope(|_world, mut images: Mut<Assets<Image>>| {
                let epoch = SessionEpoch { load_epoch: 0, tree: painted.0.screen() };
                service.tick(epoch, &mut painted.0, &mut provider, &mut runtime, &mut images);
                painted.0.update();
            });
        });
    });

    // Paint frame 1: normal painter creates an ImageNode carrying the service's actual Handle<Image>.
    app.update();
    let (bound_name, bound_handle, rev_1) = match service.diagnostic(&id) {
        RecipientDiagnostic::Bound { recipient, image_name, revision, .. } => {
            assert_eq!(recipient, recipient_tile);
            let h = app.world().resource::<RuntimeImages>().get(&image_name).unwrap().image.clone();
            (image_name, h, revision)
        }
        other => panic!("expected Bound, got {other:?}"),
    };
    assert_eq!(
        app.world().resource::<TestPainted>().0.get_string("OwnedHud/Panel/SiblingPlaceholder/filename").as_deref(),
        Some(r"Interface\Shared\solid.dds"),
        "unrelated sibling sharing solid.dds remains untouched"
    );
    assert_ne!(sibling_tile, recipient_tile);

    let nodes = |app: &mut App| {
        let w = app.world_mut();
        w.query::<(Entity, &ImageNode, &UiTransform)>()
            .iter(w)
            .map(|(e, img, tf)| (e, img.image.clone(), *tf))
            .collect::<Vec<_>>()
    };
    let painted_1 = nodes(&mut app);
    let node_1 = painted_1.iter().find(|(_, h, _)| *h == bound_handle).expect("normal painter drew bound Handle<Image>");
    assert_eq!(node_1.2, UiTransform::IDENTITY, "recipient axes (1, 0) present with zero turn without mutating XML rotateangle");
    assert_eq!(app.world().resource::<TestPainted>().0.get_float("OwnedHud/Panel/MapLeaf/rotateangle"), Some(0.5));

    // Mutate marker presentation axes to (0, 1): in-place UiTransform rotates while texture revision & Handle stay identical!
    let mut upper_same = SyntheticProvider::interior(1, [0.0, 0.0, 0.0], 1);
    app.world_mut().resource_scope(|world, mut painted: Mut<TestPainted>| {
        painted.0.set_float("OwnedHud/Panel/_AxisCos", 0.0);
        painted.0.set_float("OwnedHud/Panel/_AxisSin", 1.0);
        painted.0.update();
        world.resource_scope(|world, mut runtime: Mut<RuntimeImages>| {
            world.resource_scope(|_world, mut images: Mut<Assets<Image>>| {
                let epoch = SessionEpoch { load_epoch: 0, tree: painted.0.screen() };
                service.tick(epoch, &mut painted.0, &mut upper_same, &mut runtime, &mut images);
            });
        });
    });
    app.update();
    let painted_2 = nodes(&mut app);
    let node_2 = painted_2.iter().find(|(_, h, _)| *h == bound_handle).unwrap();
    assert_eq!(node_2.0, node_1.0, "axes change updates existing ImageNode entity in place");
    assert!((node_2.2.rotation.sin + 1.0).abs() < 1e-6 && node_2.2.rotation.cos.abs() < 1e-6);
    assert_eq!(app.world().resource::<RuntimeImages>().stamp(&bound_name).unwrap().revision, rev_1, "presentation axes do not bump texture revision");

    // Floor change mutates pixels in Assets<Image> in place on the SAME Handle<Image> and bumps RuntimeImages revision -> painter repaints!
    let mut floor_2 = SyntheticProvider::interior(1, [0.0, 0.0, 300.0], 1);
    app.world_mut().resource_scope(|world, mut painted: Mut<TestPainted>| {
        world.resource_scope(|world, mut runtime: Mut<RuntimeImages>| {
            world.resource_scope(|_world, mut images: Mut<Assets<Image>>| {
                let epoch = SessionEpoch { load_epoch: 0, tree: painted.0.screen() };
                service.tick(epoch, &mut painted.0, &mut floor_2, &mut runtime, &mut images);
                painted.0.update();
            });
        });
    });
    app.update();
    let rev_2 = app.world().resource::<RuntimeImages>().stamp(&bound_name).unwrap().revision;
    assert!(rev_2 > rev_1, "real pixel mutation advances RuntimeImages revision");
    let painted_3 = nodes(&mut app);
    let node_3 = painted_3.iter().find(|(_, h, _)| *h == bound_handle).unwrap();
    assert_ne!(node_3.0, node_2.0, "higher runtime revision repaints the normal painter node");
}

#[test]
fn placeholder_isolation_slot_reuse_external_write_protection_and_session_epoch_isolation() {
    let (mut ui, _) = load_ui(&menu_xml(""));
    let old_recipient = ui.find_tile("OwnedHud/Panel/MapLeaf").unwrap();
    let mut service = LocalMapService::default();
    let id = RecipientId::new("main_map").unwrap();
    service.register(panel_config("main_map")).unwrap();

    let mut runtime = RuntimeImages::default();
    let mut images = Assets::<Image>::default();
    let epoch = SessionEpoch { load_epoch: 0, tree: ui.screen() };
    let mut provider = SyntheticProvider::interior(1, [0.0, 0.0, 0.0], 1);

    service.tick(epoch, &mut ui, &mut provider, &mut runtime, &mut images);
    let (img_name, handle) = match service.diagnostic(&id) {
        RecipientDiagnostic::Bound { image_name, .. } => (image_name.clone(), runtime.get(&image_name).unwrap().image.clone()),
        other => panic!("expected Bound, got {other:?}"),
    };

    // Unrelated sibling sharing solid.dds is untouched.
    assert_eq!(ui.get_string("OwnedHud/Panel/SiblingPlaceholder/filename").as_deref(), Some(r"Interface\Shared\solid.dds"));

    // Independent foreign runtime alias retaining the same Handle<Image>: releasing our
    // recipient must NOT delete the image from Assets<Image>.
    runtime.set("foreign_alias", handle.clone(), Vec2::splat(8.0));

    // Unload old recipient and spawn a fresh MapLeaf into the reused numeric TileId slot:
    // generation changed -> old binding is retired and old runtime image name is removed!
    ui.unload_tile(old_recipient);
    let src = MemSource::default();
    let new_recipient = ui.add_from_template(&src, "OwnedHud/Panel", "MapLeafTpl", Some("MapLeaf")).unwrap();
    ui.update();
    assert_eq!(new_recipient.index(), old_recipient.index(), "numeric tile slot reused");
    assert_ne!(new_recipient, old_recipient, "generation-bearing TileId changed");

    // Disable local mode before ticking so the new tile does NOT get newly bound — proving
    // the reused slot NEVER silently inherits the old binding.
    ui.set_float("OwnedHud/Panel/_LocalMode", 0.0);
    ui.update();
    service.tick(epoch, &mut ui, &mut provider, &mut runtime, &mut images);
    assert!(runtime.get(&img_name).is_none(), "old owned runtime image entry removed");
    assert!(images.get(&handle).is_some(), "foreign runtime alias retains pixel asset");
    assert_eq!(
        ui.get_string("OwnedHud/Panel/MapLeaf/filename").as_deref(),
        Some(r"Interface\Shared\solid.dds"),
        "reused slot never inherits old runtime filename"
    );
    assert!(runtime.remove("foreign_alias"));

    // Re-activate on new_recipient, bind, then let an external script write _OriginX:
    // our service must detect the TraitWriteSerial conflict, refuse to clobber, and leave
    // the external write intact on release!
    ui.set_float("OwnedHud/Panel/_LocalMode", 1.0);
    ui.update();
    service.tick(epoch, &mut ui, &mut provider, &mut runtime, &mut images);
    assert!(matches!(service.diagnostic(&id), RecipientDiagnostic::Bound { recipient, .. } if recipient == new_recipient));

    ui.set_float("OwnedHud/Panel/_OriginX", 4242.0);
    ui.update();
    service.tick(epoch, &mut ui, &mut provider, &mut runtime, &mut images);
    assert!(matches!(
        service.diagnostic(&id),
        RecipientDiagnostic::CompletedUnbound { reason: BindFailure::SinkChanged, .. }
    ));
    assert_eq!(ui.get_float("OwnedHud/Panel/_OriginX"), Some(4242.0), "external script write never clobbered");
    assert_eq!(ui.get_string("OwnedHud/Panel/MapLeaf/filename").as_deref(), Some(r"Interface\Shared\solid.dds"), "unmodified filename sink cleanly restored");

    // Committed session rollover (load_epoch 0 -> 1) retires all bindings and cancels work cleanly.
    ui.set_float("OwnedHud/Panel/_ForceUpdate", 1.0);
    ui.update();
    service.tick(SessionEpoch { load_epoch: 1, tree: ui.screen() }, &mut ui, &mut provider, &mut runtime, &mut images);
    let post_load_name = match service.diagnostic(&id) {
        RecipientDiagnostic::Bound { image_name, .. } => image_name,
        other => panic!("expected fresh Bound in new session, got {other:?}"),
    };
    assert_ne!(post_load_name, img_name, "new session allocates fresh runtime image name");

    let (mut fresh_ui, _) = load_ui(&menu_xml(""));
    fresh_ui.set_float("OwnedHud/Panel/_LocalMode", 0.0);
    fresh_ui.update();
    service.tick(SessionEpoch { load_epoch: 2, tree: fresh_ui.screen() }, &mut fresh_ui, &mut provider, &mut runtime, &mut images);
    assert!(runtime.get(&post_load_name).is_none(), "previous session runtime image removed on new session");
    assert_eq!(fresh_ui.get_string("OwnedHud/Panel/MapLeaf/filename").as_deref(), Some(r"Interface\Shared\solid.dds"));
}

#[test]
fn diagnostics_distinguish_provider_absence_loading_invalid_pending_unbound_and_bound() {
    let (mut ui, _) = load_ui(&menu_xml(""));
    let mut service = LocalMapService::with_budget(ServiceBudget { max_rays_per_frame: 8, max_rays_per_batch: 8, ..Default::default() });
    let id = RecipientId::new("diag_map").unwrap();
    assert_eq!(service.diagnostic(&id), RecipientDiagnostic::Unregistered);
    service.register(panel_config("diag_map")).unwrap();

    let mut runtime = RuntimeImages::default();
    let mut images = Assets::<Image>::default();
    let epoch = SessionEpoch { load_epoch: 0, tree: ui.screen() };

    // 1. ProviderUnavailable
    let mut unavail = SyntheticProvider { state: ProviderState::Unavailable, rays: 0 };
    service.tick(epoch, &mut ui, &mut unavail, &mut runtime, &mut images);
    assert_eq!(service.diagnostic(&id), RecipientDiagnostic::ProviderUnavailable);

    // 2. ProviderLoading (transient loading gate)
    let mut loading = SyntheticProvider { state: ProviderState::Loading, rays: 0 };
    service.tick(epoch, &mut ui, &mut loading, &mut runtime, &mut images);
    assert_eq!(service.diagnostic(&id), RecipientDiagnostic::ProviderLoading);

    // 3. Pending under tight ray budget
    let mut ready = SyntheticProvider::interior(1, [0.0, 0.0, 0.0], 1);
    service.tick(epoch, &mut ui, &mut ready, &mut runtime, &mut images);
    assert!(matches!(service.diagnostic(&id), RecipientDiagnostic::Pending { sampled: 8, total, .. } if total > 8));

    // 4. Advance until Bound
    for _ in 0..64 {
        service.tick(epoch, &mut ui, &mut ready, &mut runtime, &mut images);
        if matches!(service.diagnostic(&id), RecipientDiagnostic::Bound { .. }) {
            break;
        }
    }
    assert!(matches!(service.diagnostic(&id), RecipientDiagnostic::Bound { .. }));

    // 5. InvalidRequest (oversized / NaN / expression-driven sink)
    ui.set_float("OwnedHud/Panel/_Scale", f64::NAN);
    ui.update();
    service.tick(epoch, &mut ui, &mut ready, &mut runtime, &mut images);
    assert_eq!(service.diagnostic(&id), RecipientDiagnostic::InvalidRequest(RequestRejection::InvalidFieldValue));

    // 6. CompletedUnbound via external sink conflict
    ui.set_float("OwnedHud/Panel/_Scale", 8.0);
    ui.update();
    for _ in 0..64 {
        service.tick(epoch, &mut ui, &mut ready, &mut runtime, &mut images);
        if matches!(service.diagnostic(&id), RecipientDiagnostic::Bound { .. }) {
            break;
        }
    }
    ui.set_float("OwnedHud/Panel/_OriginY", 1234.0);
    ui.update();
    service.tick(epoch, &mut ui, &mut ready, &mut runtime, &mut images);
    assert!(matches!(
        service.diagnostic(&id),
        RecipientDiagnostic::CompletedUnbound { reason: BindFailure::SinkChanged, .. }
    ));
}

#[test]
fn global_shared_budget_backpressure_invalid_requests_and_idle_zero_work_no_op() {
    // 1. Empty registry (default off): strictly zero rays, zero images, zero UI writes.
    let (mut empty_ui, _) = load_ui(&menu_xml(""));
    let writes_before = empty_ui.trait_write_counts();
    let mut empty_svc = LocalMapService::default();
    let mut runtime = RuntimeImages::default();
    let mut images = Assets::<Image>::default();
    let mut provider = SyntheticProvider::interior(1, [0.0, 0.0, 0.0], 1);
    let epoch = SessionEpoch { load_epoch: 0, tree: empty_ui.screen() };
    empty_svc.tick(epoch, &mut empty_ui, &mut provider, &mut runtime, &mut images);
    assert_eq!(provider.rays, 0);
    assert_eq!(empty_ui.trait_write_counts(), writes_before);
    assert_eq!(empty_svc.counters().idle_frames, 1);
    assert_eq!(empty_svc.counters().rays_cast, 0);
    assert_eq!(images.len(), 0);

    // 2. Two recipients competing for one shared ray budget & resident image/pixel cap.
    let second_panel = r#"
    <rect name="PanelB">
      <_LocalMode>1</_LocalMode>
      <_ForceUpdate>0</_ForceUpdate>
      <_WinW>40</_WinW>
      <_WinH>30</_WinH>
      <_Scale>8</_Scale>
      <_BasisAngle>0</_BasisAngle>
      <_AxisCos>1</_AxisCos>
      <_AxisSin>0</_AxisSin>
      <_OriginX>-999999</_OriginX>
      <_OriginY>-999999</_OriginY>
      <_SpanX>0</_SpanX>
      <_SpanY>0</_SpanY>
      <image name="MapLeafB">
        <filename>Interface\Shared\solid.dds</filename>
        <width>64</width>
        <height>48</height>
        <visible>1</visible>
      </image>
    </rect>"#;
    let (mut ui, _) = load_ui(&menu_xml(second_panel));
    let mut svc = LocalMapService::with_budget(ServiceBudget {
        max_recipients: 2,
        max_rays_per_frame: 24,
        max_rays_per_batch: 12,
        max_resident_images: 1,
        max_pending_jobs: 2,
        max_edge: 128,
        max_pixels_per_image: 128 * 128,
        ..Default::default()
    });
    let id_a = RecipientId::new("a").unwrap();
    let id_b = RecipientId::new("b").unwrap();
    let id_c = RecipientId::new("c").unwrap();
    svc.register(panel_config("a")).unwrap();
    svc.register(RecipientConfig {
        id: id_b.clone(),
        source_path: "OwnedHud/PanelB".into(),
        recipient_path: "OwnedHud/PanelB/MapLeafB".into(),
        fields: standard_fields(),
    })
    .unwrap();
    assert_eq!(svc.register(panel_config("c")), Err(RegisterError::RecipientLimit), "max_recipients enforced");
    assert_eq!(svc.diagnostic(&id_c), RecipientDiagnostic::Unregistered);

    let epoch = SessionEpoch { load_epoch: 0, tree: ui.screen() };
    let mut provider = SyntheticProvider::interior(1, [0.0, 0.0, 0.0], 1);
    svc.tick(epoch, &mut ui, &mut provider, &mut runtime, &mut images);
    assert!(provider.rays <= 24, "shared global ray cap strictly honored across both recipients: {}", provider.rays);

    // Advance until A binds; B must then hit ResidentImageCap (max_resident_images = 1)!
    for _ in 0..30 {
        svc.tick(epoch, &mut ui, &mut provider, &mut runtime, &mut images);
    }
    assert!(matches!(svc.diagnostic(&id_a), RecipientDiagnostic::Bound { .. }));
    // Trigger B re-check so B observes resident cap backpressure cleanly.
    ui.set_float("OwnedHud/PanelB/_Scale", 12.0);
    ui.update();
    svc.tick(epoch, &mut ui, &mut provider, &mut runtime, &mut images);
    assert_eq!(svc.diagnostic(&id_b), RecipientDiagnostic::InvalidRequest(RequestRejection::ResidentImageCap));
    assert!(svc.counters().backpressure_rejections >= 1);

    // Deactivate B so only A is active, then verify an oversized request on A is rejected
    // BEFORE allocation and casts strictly zero rays.
    ui.set_float("OwnedHud/PanelB/_LocalMode", 0.0);
    ui.set_float("OwnedHud/Panel/_Scale", 5000.0);
    ui.update();
    let rays_before = provider.rays;
    svc.tick(epoch, &mut ui, &mut provider, &mut runtime, &mut images);
    assert_eq!(svc.diagnostic(&id_a), RecipientDiagnostic::InvalidRequest(RequestRejection::Grid(GridRejected::PixelLimit)));
    assert_eq!(provider.rays, rays_before, "oversized request casts zero rays");

    // Once settled and unchanged, subsequent ticks are idle zero-work no-ops.
    ui.set_float("OwnedHud/Panel/_Scale", 8.0);
    ui.update();
    for _ in 0..30 {
        svc.tick(epoch, &mut ui, &mut provider, &mut runtime, &mut images);
    }
    assert!(matches!(svc.diagnostic(&id_a), RecipientDiagnostic::Bound { .. }));
    let rays_settled = provider.rays;
    let writes_settled = ui.trait_write_counts();
    let idle_before = svc.counters().idle_frames;
    svc.tick(epoch, &mut ui, &mut provider, &mut runtime, &mut images);
    assert_eq!(provider.rays, rays_settled, "settled frame casts zero rays");
    assert_eq!(ui.trait_write_counts(), writes_settled, "settled frame performs zero UI writes");
    assert_eq!(svc.counters().idle_frames, idle_before + 1);
}
