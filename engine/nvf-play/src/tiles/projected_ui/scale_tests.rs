//! Scoped regression tests verifying that off-screen projected UI targets lay out
//! `1 interface unit == 1 target pixel` regardless of viewport height or user UI/menu
//! scale (preventing shrunk top-left rendering, black borders, and shifted border bars
//! on non-960px projected targets such as 768x576 Pip-Boy wrist projections).

use super::*;
use crate::tiles::draw::TileFonts;
use crate::tiles::runtime_images::RuntimeImages;
use fnv_sim::FormId;
use fnv_ui::MemSource;

fn vh_to_px(val: Val, target_height: f32) -> f32 {
    match val {
        Val::Vh(v) => v * target_height / 100.0,
        other => panic!("expected Val::Vh, got {other:?}"),
    }
}

fn projection_app(xml: &str) -> App {
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
        .insert_resource(ProjectedUiTargets::default())
        .insert_resource(script)
        .add_systems(Update, (reconcile, draw).chain());
    app
}

#[test]
fn projected_768x576_viewport_maps_interface_units_one_to_one_in_pixels() {
    let xml = r#"<menu name="HUDMainMenu">
  <rect name="WristRoot">
    <visible>1</visible>
    <x>-15000</x>
    <y>-15000</y>
    <width>768</width>
    <height>576</height>
    <rect name="EdgeBar">
      <x>547</x>
      <y>411</y>
      <width>26</width>
      <height>26</height>
      <red>255</red><green>0</green><blue>0</blue>
    </rect>
  </rect>
</menu>"#;
    let mut app = projection_app(xml);
    {
        let mut script = app.world_mut().resource_mut::<ScriptState>();
        let rt = script.rt.as_mut().unwrap();
        rt.game
            .ui_extra
            .projections
            .set(&rt.game.ui_tree, FormId(0x14), "HUDMainMenu/WristRoot", "pipboyscreen:0", [-15000.0, -15000.0, 768.0, 576.0])
            .unwrap();
    }

    // Even when an outer non-1.0 unit scale is active on the thread, projected targets must
    // still resolve 1 interface unit == 1 target pixel inside 768x576.
    {
        let _outer = crate::interface::units_at(0.75);
        app.update();
    }

    let world = app.world_mut();
    let root_node = world.query_filtered::<&Node, With<UiTargetCamera>>().single(world).expect("projected root node");
    assert!((vh_to_px(root_node.width, 576.0) - 768.0).abs() < 1e-3, "root width must be 768px, got {:?}", root_node.width);
    assert!((vh_to_px(root_node.height, 576.0) - 576.0).abs() < 1e-3, "root height must be 576px, got {:?}", root_node.height);

    let bar_node = world
        .query::<(&Node, &BackgroundColor)>()
        .iter(world)
        .find_map(|(n, bg)| {
            let s = bg.0.to_srgba();
            (s.red > 0.5 && s.green < 0.1).then_some(n)
        })
        .expect("projected red EdgeBar fill node");
    assert!((vh_to_px(bar_node.left, 576.0) - 547.0).abs() < 1e-3, "EdgeBar left must be 547px, got {:?}", bar_node.left);
    assert!((vh_to_px(bar_node.top, 576.0) - 411.0).abs() < 1e-3, "EdgeBar top must be 411px, got {:?}", bar_node.top);
    assert!((vh_to_px(bar_node.width, 576.0) - 26.0).abs() < 1e-3, "EdgeBar width must be 26px, got {:?}", bar_node.width);
    assert!((vh_to_px(bar_node.height, 576.0) - 26.0).abs() < 1e-3, "EdgeBar height must be 26px, got {:?}", bar_node.height);

    // Resizing the projection viewport to 512x384 updates the Val::Vh scale cleanly to stay 1:1.
    {
        let mut script = app.world_mut().resource_mut::<ScriptState>();
        let rt = script.rt.as_mut().unwrap();
        rt.game
            .ui_extra
            .projections
            .set(&rt.game.ui_tree, FormId(0x14), "HUDMainMenu/WristRoot", "pipboyscreen:0", [-15000.0, -15000.0, 512.0, 384.0])
            .unwrap();
    }
    app.update();

    let world = app.world_mut();
    let resized_root = world.query_filtered::<&Node, With<UiTargetCamera>>().single(world).expect("resized root node");
    assert!((vh_to_px(resized_root.width, 384.0) - 512.0).abs() < 1e-3);
    assert!((vh_to_px(resized_root.height, 384.0) - 384.0).abs() < 1e-3);
}
