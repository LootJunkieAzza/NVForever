//! CPU/ECS acceptance tests for projected-UI painting and LocalMapService integration
//! (Part of #2418; docs/spec-notes/local-map-ui-service.md,
//! docs/spec-notes/held-ui-projection-state.md).

use super::*;
use crate::tiles::draw::{self, TileFonts};
use crate::tiles::local_map_service::{
    FieldBinding, LocalMapProvider, LocalMapService, ProviderSnapshot, ProviderState, RecipientConfig,
    RecipientDiagnostic, RecipientId, SessionEpoch,
};
use crate::tiles::runtime_images::RuntimeImages;
use fnv_sim::FormId;
use fnv_ui::MemSource;

fn projected_local_map_xml() -> &'static str {
    r#"<menu name="HUDMainMenu">
  <rect name="HiddenHost">
    <visible>0</visible>
    <rect name="HeldMapRoot">
      <visible>1</visible>
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
      <image name="MapLeaf">
        <filename>Interface\Shared\solid.dds</filename>
        <x>10</x>
        <y>20</y>
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
  </rect>
</menu>"#
}

fn held_fields() -> FieldBinding {
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

struct FloorProvider {
    feet: [f32; 3],
}

impl LocalMapProvider for FloorProvider {
    fn state(&self) -> ProviderState {
        ProviderState::Ready(ProviderSnapshot { interior: Some(1), world: 0, feet: self.feet, static_rev: 1 })
    }

    fn sample_ray(&mut self, origin: [f32; 3], _reach: f32) -> Option<f32> {
        if self.feet[2] >= 200.0 {
            (origin[0] >= 10.0 && origin[0] <= 160.0 && origin[1] <= -10.0 && origin[1] >= -140.0)
                .then_some(self.feet[2])
        } else {
            (origin[0] >= -160.0 && origin[0] <= -10.0 && origin[1] >= 10.0 && origin[1] <= 140.0)
                .then_some(0.0)
        }
    }
}

fn test_app(xml: &str, budget: ProjectionBudget) -> App {
    let mut script = ScriptState::over_no_plugins();
    {
        let rt = script.rt.as_mut().unwrap();
        let src = MemSource::default().with("hud_main_menu.xml", xml);
        rt.game.ui_tree.load_menu(&src, "hud_main_menu.xml").unwrap();
        rt.game.ui_tree.update();
    }
    let mut app = App::new();
    app.init_resource::<Assets<Image>>()
        .init_resource::<TileFonts>()
        .init_resource::<crate::pad::PadGlyphs>()
        .init_resource::<RuntimeImages>()
        .insert_resource(ProjectedUiTargets::with_budget(budget))
        .insert_resource(script)
        .add_systems(Update, (reconcile, draw).chain());
    app
}

fn painted_image_nodes(app: &mut App) -> Vec<(Entity, Handle<Image>, UiTransform)> {
    let w = app.world_mut();
    w.query::<(Entity, &ImageNode, &UiTransform)>()
        .iter(w)
        .map(|(e, img, tf)| (e, img.image.clone(), *tf))
        .collect()
}

fn camera_entities(app: &mut App) -> Vec<Entity> {
    let w = app.world_mut();
    w.query_filtered::<Entity, With<Camera2d>>().iter(w).collect()
}

#[test]
fn local_map_service_assets_image_mutation_revision_and_axes_reach_projected_painter() {
    let mut app = test_app(projected_local_map_xml(), ProjectionBudget::default());

    let mut service = LocalMapService::default();
    let id = RecipientId::new("held_local_map").unwrap();
    service
        .register(RecipientConfig {
            id: id.clone(),
            source_path: "HUDMainMenu/HiddenHost/HeldMapRoot".into(),
            recipient_path: "HUDMainMenu/HiddenHost/HeldMapRoot/MapLeaf".into(),
            fields: held_fields(),
        })
        .unwrap();

    // Register a ProjectUITile request targeting the isolated subtree under the hidden host.
    let target = Target { reference: FormId(0x14), block: "mapscreen".into() };
    let recipient_tile = {
        let mut script = app.world_mut().resource_mut::<ScriptState>();
        let rt = script.rt.as_mut().unwrap();
        rt.game
            .ui_extra
            .projections
            .set(&rt.game.ui_tree, FormId(0x14), "HUDMainMenu/HiddenHost/HeldMapRoot", "MapScreen", [0.0, 0.0, 128.0, 96.0])
            .unwrap();
        rt.game.ui_tree.find_tile("HUDMainMenu/HiddenHost/HeldMapRoot/MapLeaf").unwrap()
    };

    // Note: LocalMapService checks `shown(ui, recipient)`, which requires the recipient to be
    // visible. Make the host visible only when ticking LocalMapService? No — check that an
    // active visible root under a visible host works, OR prove hidden-ancestor isolation in its
    // own dedicated test below. Let's verify how `shown` behaves: `HiddenHost` has `<visible>0</visible>`,
    // so `LocalMapService` sees `Inactive` unless the host is visible! Set HiddenHost visible=1 here
    // so LocalMapService admits it cleanly, and test hidden-ancestor isolation in the dedicated test!
    {
        let mut script = app.world_mut().resource_mut::<ScriptState>();
        let rt = script.rt.as_mut().unwrap();
        rt.game.ui_tree.set_float("HUDMainMenu/HiddenHost/visible", 1.0);
        rt.game.ui_tree.update();
    }

    // Frame 1: tick LocalMapService on ground floor, evaluate UI, then run projected painter.
    let mut floor_1 = FloorProvider { feet: [0.0, 0.0, 0.0] };
    app.world_mut().resource_scope(|world, mut script: Mut<ScriptState>| {
        world.resource_scope(|world, mut runtime: Mut<RuntimeImages>| {
            world.resource_scope(|_world, mut images: Mut<Assets<Image>>| {
                let rt = script.rt.as_mut().unwrap();
                let epoch = SessionEpoch { load_epoch: rt.session_epoch, tree: rt.game.ui_tree.screen() };
                service.tick(epoch, &mut rt.game.ui_tree, &mut floor_1, &mut runtime, &mut images);
                rt.game.ui_tree.update();
            });
        });
    });

    app.update();

    let (bound_name, bound_handle, rev_1) = match service.diagnostic(&id) {
        RecipientDiagnostic::Bound { recipient, image_name, revision, .. } => {
            assert_eq!(recipient, recipient_tile);
            let h = app.world().resource::<RuntimeImages>().get(&image_name).unwrap().image.clone();
            (image_name, h, revision)
        }
        other => panic!("expected Bound, got {other:?}"),
    };

    let output = app.world().resource::<ProjectedUiTargets>().get(&target).expect("projected target active").clone();
    assert_eq!(output.viewport.width, 128);
    assert_eq!(output.viewport.height, 96);
    assert!(app.world().resource::<Assets<Image>>().get(&output.image).is_some());

    // Projected painter created an ImageNode carrying LocalMapService's exact Handle<Image>
    // under UiTargetCamera(output.camera), while unrelated SiblingPlaceholder stayed solid.dds.
    let nodes_1 = painted_image_nodes(&mut app);
    let node_1 = nodes_1
        .iter()
        .find(|(_, h, _)| *h == bound_handle)
        .expect("projected painter drew LocalMapService's actual Handle<Image>");
    assert_eq!(node_1.2, UiTransform::IDENTITY, "recipient axes (1, 0) present without mutating XML rotateangle");
    assert_eq!(
        app.world().resource::<ScriptState>().rt.as_ref().unwrap().game.ui_tree.get_float("HUDMainMenu/HiddenHost/HeldMapRoot/MapLeaf/rotateangle"),
        Some(0.5)
    );
    assert_eq!(
        app.world().resource::<ScriptState>().rt.as_ref().unwrap().game.ui_tree.get_string("HUDMainMenu/HiddenHost/HeldMapRoot/SiblingPlaceholder/filename").as_deref(),
        Some(r"Interface\Shared\solid.dds")
    );

    // Frame 2: rotate marker presentation axes to (0, 1) -> projected ImageNode updates in place
    // without bumping texture revision!
    let mut floor_1_same = FloorProvider { feet: [0.0, 0.0, 0.0] };
    app.world_mut().resource_scope(|world, mut script: Mut<ScriptState>| {
        let rt = script.rt.as_mut().unwrap();
        rt.game.ui_tree.set_float("HUDMainMenu/HiddenHost/HeldMapRoot/_AxisCos", 0.0);
        rt.game.ui_tree.set_float("HUDMainMenu/HiddenHost/HeldMapRoot/_AxisSin", 1.0);
        rt.game.ui_tree.update();
        world.resource_scope(|world, mut runtime: Mut<RuntimeImages>| {
            world.resource_scope(|_world, mut images: Mut<Assets<Image>>| {
                let epoch = SessionEpoch { load_epoch: rt.session_epoch, tree: rt.game.ui_tree.screen() };
                service.tick(epoch, &mut rt.game.ui_tree, &mut floor_1_same, &mut runtime, &mut images);
            });
        });
    });
    app.update();

    let nodes_2 = painted_image_nodes(&mut app);
    let node_2 = nodes_2.iter().find(|(_, h, _)| *h == bound_handle).unwrap();
    assert_eq!(node_2.0, node_1.0, "axes change updates projected ImageNode entity in place");
    assert!((node_2.2.rotation.sin + 1.0).abs() < 1e-6 && node_2.2.rotation.cos.abs() < 1e-6);
    assert_eq!(app.world().resource::<RuntimeImages>().stamp(&bound_name).unwrap().revision, rev_1);

    // Frame 3: upper floor mutates Assets<Image> in place on the SAME Handle<Image> and bumps
    // RuntimeImages revision -> projected painter repaints the node!
    let mut floor_2 = FloorProvider { feet: [0.0, 0.0, 300.0] };
    app.world_mut().resource_scope(|world, mut script: Mut<ScriptState>| {
        world.resource_scope(|world, mut runtime: Mut<RuntimeImages>| {
            world.resource_scope(|_world, mut images: Mut<Assets<Image>>| {
                let rt = script.rt.as_mut().unwrap();
                let epoch = SessionEpoch { load_epoch: rt.session_epoch, tree: rt.game.ui_tree.screen() };
                service.tick(epoch, &mut rt.game.ui_tree, &mut floor_2, &mut runtime, &mut images);
                rt.game.ui_tree.update();
            });
        });
    });
    app.update();

    let rev_2 = app.world().resource::<RuntimeImages>().stamp(&bound_name).unwrap().revision;
    assert!(rev_2 > rev_1, "real Assets<Image> mutation bumps RuntimeImages revision");
    let nodes_3 = painted_image_nodes(&mut app);
    let node_3 = nodes_3.iter().find(|(_, h, _)| *h == bound_handle).unwrap();
    assert_ne!(node_3.0, node_2.0, "higher runtime revision repaints the projected painter node");
}

#[test]
fn viewport_offset_clip_validation_and_hidden_ancestor_isolation() {
    let xml = r#"<menu name="HUDMainMenu">
  <rect name="HiddenOuter">
    <visible>0</visible>
    <x>500</x>
    <y>400</y>
    <rect name="ProjRoot">
      <visible>1</visible>
      <clips>1</clips>
      <x>-40</x>
      <y>-20</y>
      <width>100</width>
      <height>60</height>
      <rect name="Inside">
        <x>10</x>
        <y>5</y>
        <width>30</width>
        <height>20</height>
        <red>255</red><green>128</green><blue>0</blue>
      </rect>
      <rect name="ClippedOff">
        <clips>1</clips>
        <x>200</x>
        <y>200</y>
        <width>20</width>
        <height>20</height>
        <rect name="OffChild">
          <width>10</width>
          <height>10</height>
          <red>255</red><green>0</green><blue>0</blue>
        </rect>
      </rect>
    </rect>
  </rect>
</menu>"#;
    let src = MemSource::default().with("hud_main_menu.xml", xml);
    let mut ui = Ui::new();
    let menu = ui.load_menu(&src, "hud_main_menu.xml").unwrap();
    ui.update();

    let measure = |_: f64, _: &str, _: Option<f64>| (0.0, 0.0);
    // Ordinary menu layout hides everything under HiddenOuter.
    let (normal, _) = draw::pieces(&ui, menu, &measure, draw::Select::Whole { kept: &[], strip: None }, true);
    assert!(normal.is_empty(), "normal HUD draws nothing under hidden ancestor");

    // Projected layout isolates ProjRoot from HiddenOuter's visibility and (500, 400) offset,
    // shifts by (-viewport.x, -viewport.y), and drops pieces clipped outside the viewport.
    let root = ui.find_tile("HUDMainMenu/HiddenOuter/ProjRoot").unwrap();
    let vp = Viewport { x: -40.0, y: -20.0, width: 80, height: 40 };
    let pieces = draw::projected_pieces(&ui, root, vp, &measure, true);
    assert_eq!(pieces.len(), 1, "only Inside survives viewport clip intersection: {pieces:?}");
    assert_eq!(pieces[0].tile, ui.find_tile("HUDMainMenu/HiddenOuter/ProjRoot/Inside").unwrap());
    assert_eq!(pieces[0].rect.left, 10.0);
    assert_eq!(pieces[0].rect.top, 5.0);
    let clip = pieces[0].clip.expect("clip retained and clamped to viewport");
    assert_eq!((clip.left, clip.top, clip.width, clip.height), (0.0, 0.0, 80.0, 40.0));

    // Hiding ProjRoot itself produces zero projected pieces.
    ui.set_float("HUDMainMenu/HiddenOuter/ProjRoot/visible", 0.0);
    ui.update();
    assert!(draw::projected_pieces(&ui, root, vp, &measure, true).is_empty());
}

#[test]
fn withdrawal_resize_stale_source_and_session_epoch_clean_up_cameras_and_images() {
    let xml = r#"<menu name="HUDMainMenu">
  <template name="BoxTpl">
    <rect name="Box">
      <width>32</width><height>32</height>
      <red>255</red><green>200</green><blue>100</blue>
    </rect>
  </template>
  <rect name="Box">
    <width>32</width><height>32</height>
    <red>255</red><green>200</green><blue>100</blue>
  </rect>
</menu>"#;
    let mut app = test_app(xml, ProjectionBudget::default());
    let target = Target { reference: FormId(0x20), block: "screen".into() };

    // 1. Register and paint initial 64x64 target.
    {
        let mut script = app.world_mut().resource_mut::<ScriptState>();
        let rt = script.rt.as_mut().unwrap();
        rt.game.ui_extra.projections.set(&rt.game.ui_tree, FormId(0x20), "HUDMainMenu/Box", "screen", [0.0, 0.0, 64.0, 64.0]).unwrap();
    }
    app.update();
    let img_1 = app.world().resource::<ProjectedUiTargets>().get(&target).unwrap().image.clone();
    assert_eq!(camera_entities(&mut app).len(), 1);
    assert!(app.world().resource::<Assets<Image>>().get(&img_1).is_some());

    // 2. Resize viewport to 128x96 -> replaces target image and removes old Handle<Image>.
    {
        let mut script = app.world_mut().resource_mut::<ScriptState>();
        let rt = script.rt.as_mut().unwrap();
        rt.game.ui_extra.projections.set(&rt.game.ui_tree, FormId(0x20), "HUDMainMenu/Box", "screen", [0.0, 0.0, 128.0, 96.0]).unwrap();
    }
    app.update();
    let img_2 = app.world().resource::<ProjectedUiTargets>().get(&target).unwrap().image.clone();
    assert_ne!(img_1, img_2);
    assert!(app.world().resource::<Assets<Image>>().get(&img_1).is_none(), "old target image freed on resize");
    assert!(app.world().resource::<Assets<Image>>().get(&img_2).is_some());
    assert_eq!(app.world().resource::<ProjectedUiTargets>().counters().targets_resized, 1);

    // 3. Exact 1x1 withdrawal -> despawns camera and frees target image.
    {
        let mut script = app.world_mut().resource_mut::<ScriptState>();
        let rt = script.rt.as_mut().unwrap();
        rt.game.ui_extra.projections.set(&rt.game.ui_tree, FormId(0x20), "HUDMainMenu/Box", "screen", [0.0, 0.0, 1.0, 1.0]).unwrap();
    }
    app.update();
    assert!(app.world().resource::<ProjectedUiTargets>().is_empty());
    assert!(camera_entities(&mut app).is_empty(), "withdrawn camera despawned");
    assert!(app.world().resource::<Assets<Image>>().get(&img_2).is_none(), "withdrawn image freed");

    // 4. Unloading source and reusing its numeric TileId slot retires projection via reconcile.
    {
        let mut script = app.world_mut().resource_mut::<ScriptState>();
        let rt = script.rt.as_mut().unwrap();
        rt.game.ui_extra.projections.set(&rt.game.ui_tree, FormId(0x20), "HUDMainMenu/Box", "screen", [0.0, 0.0, 48.0, 48.0]).unwrap();
    }
    app.update();
    let img_3 = app.world().resource::<ProjectedUiTargets>().get(&target).unwrap().image.clone();
    {
        let mut script = app.world_mut().resource_mut::<ScriptState>();
        let rt = script.rt.as_mut().unwrap();
        let old = rt.game.ui_tree.find_tile("HUDMainMenu/Box").unwrap();
        rt.game.ui_tree.unload_tile(old);
        let replaced = rt.game.ui_tree.add_from_template(&MemSource::default(), "HUDMainMenu", "BoxTpl", Some("Box")).unwrap();
        rt.game.ui_tree.update();
        assert_eq!(old.index(), replaced.index(), "numeric slot reused");
        assert_ne!(old, replaced, "generation advanced");
    }
    app.update();
    assert!(app.world().resource::<ProjectedUiTargets>().is_empty(), "stale generation retired by reconcile");
    assert!(camera_entities(&mut app).is_empty());
    assert!(app.world().resource::<Assets<Image>>().get(&img_3).is_none());

    // 5. Committed SessionEpoch rollover tears down previous session targets cleanly.
    {
        let mut script = app.world_mut().resource_mut::<ScriptState>();
        let rt = script.rt.as_mut().unwrap();
        rt.game.ui_extra.projections.set(&rt.game.ui_tree, FormId(0x20), "HUDMainMenu/Box", "screen", [0.0, 0.0, 48.0, 48.0]).unwrap();
    }
    app.update();
    let img_4 = app.world().resource::<ProjectedUiTargets>().get(&target).unwrap().image.clone();
    {
        let mut script = app.world_mut().resource_mut::<ScriptState>();
        let rt = script.rt.as_mut().unwrap();
        rt.session_epoch += 1;
        rt.game.ui_extra.projections.clear();
    }
    app.update();
    assert!(app.world().resource::<ProjectedUiTargets>().is_empty());
    assert!(camera_entities(&mut app).is_empty());
    assert!(app.world().resource::<Assets<Image>>().get(&img_4).is_none());
}

#[test]
fn idle_zero_work_no_op_and_independent_painter_budget_caps() {
    let xml = r#"<menu name="HUDMainMenu"><rect name="Box"><width>32</width><height>32</height><red>255</red></rect></menu>"#;
    let budget = ProjectionBudget { max_targets: 1, max_edge: 128, max_total_pixels: 64 * 64 };
    let mut app = test_app(xml, budget);

    // Idle frames with no requests perform zero allocations/cameras/images.
    app.update();
    app.update();
    let c = app.world().resource::<ProjectedUiTargets>().counters();
    assert_eq!(c.idle_frames, 2);
    assert_eq!(c.targets_created, 0);
    assert!(camera_entities(&mut app).is_empty());
    assert_eq!(app.world().resource::<Assets<Image>>().len(), 0);

    // Admit two requests in ui_projection::Requests: painter cap (max_targets = 1) accepts the
    // first and reports distinct RejectedBudget for the second without allocating a second camera.
    let t1 = Target { reference: FormId(1), block: "screen".into() };
    let t2 = Target { reference: FormId(2), block: "screen".into() };
    {
        let mut script = app.world_mut().resource_mut::<ScriptState>();
        let rt = script.rt.as_mut().unwrap();
        rt.game.ui_extra.projections.set(&rt.game.ui_tree, FormId(1), "HUDMainMenu/Box", "screen", [0.0, 0.0, 64.0, 64.0]).unwrap();
        rt.game.ui_extra.projections.set(&rt.game.ui_tree, FormId(2), "HUDMainMenu/Box", "screen", [0.0, 0.0, 64.0, 64.0]).unwrap();
    }
    app.update();
    let targets = app.world().resource::<ProjectedUiTargets>();
    assert!(matches!(targets.diagnostic(&t1), ProjectionDiagnostic::Active { .. }));
    assert_eq!(targets.diagnostic(&t2), ProjectionDiagnostic::RejectedBudget);
    assert_eq!(targets.counters().budget_rejections, 1);
    assert_eq!(camera_entities(&mut app).len(), 1);
}
