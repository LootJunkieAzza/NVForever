//! Turned and runtime pictures through the real painting and Bevy's own UI layout, at a
//! fractional scale (0.75 pixels a unit) [design]. The harness follows
//! `crate::interface`'s layout tests (Bevy's own `bevy_ui::layout` tests): no window,
//! renderer or picking. As the Pip-Boy's image (`crate::pipboy::PipBoyScreen::target`), the
//! camera's image is one logical pixel an interface unit: 1280 x 720 pixels at a scale
//! factor of 0.75 ([`SCALE`]).

use super::*;
use bevy::app::{HierarchyPropagatePlugin, PostUpdate, PropagateSet, TaskPoolPlugin};
use bevy::asset::RenderAssetUsages;
use bevy::camera::{Camera, Camera2d, ComputedCameraValues, RenderTargetInfo};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::text::{FontCx, RemSize, ScaleCx, TextPipeline};
use bevy::ui::ui_surface::UiSurface;
use bevy::ui::update::propagate_ui_target_cameras;
use bevy::ui::{sync_font_size_to_em_size, ui_layout_system, ComputedNode, ComputedUiRenderTargetInfo, ComputedUiTargetCamera, UiGlobalTransform, UiScale};
use fnv_ui::MemSource;

use super::super::runtime_images::RuntimeImages;

/// 720 lines: an interface unit (960 to the height) is 0.75 of a pixel.
const VIEW: UVec2 = UVec2::new(1280, 720);
const PX: f64 = 720.0 / 960.0;
/// The image's pixels to a logical pixel (one logical pixel an interface unit).
const SCALE: f32 = 0.75;

#[derive(Resource)]
struct Tree(Ui, TileId);

#[derive(Resource)]
struct Canvas(Entity);

/// Paint the tree's whole menu into the camera's image, as the Pip-Boy's pages are painted.
fn paint_tree(mut commands: Commands, tree: Res<Tree>, canvas: Res<Canvas>, mut kit: FontKit, mut nodes: Nodes, glyphs: Res<crate::pad::PadGlyphs>, mut drawn: Local<Drawn>) {
    let (set, _) = pieces(&tree.0, tree.1, &|_, _, _| (0.0, 0.0), Select::Whole { kept: &[], strip: None }, true);
    let target = Target::image(canvas.0, Vec2::new(1706.0, 960.0), SCALE, 1);
    paint(&mut commands, &mut drawn, set, target, 0.0, Some(&tree.0), &mut kit, &mut nodes, &glyphs);
}

fn app(xml: &str) -> App {
    let src = MemSource::default().with("t.xml", xml);
    let mut ui = Ui::new();
    let menu = ui.load_menu(&src, "t.xml").unwrap();
    ui.update();
    let mut app = App::new();
    app.add_plugins(TaskPoolPlugin::default());
    app.add_plugins(HierarchyPropagatePlugin::<ComputedUiTargetCamera>::new(PostUpdate));
    app.add_plugins(HierarchyPropagatePlugin::<ComputedUiRenderTargetInfo>::new(PostUpdate));
    app.init_resource::<UiScale>()
        .init_resource::<UiSurface>()
        .init_resource::<TextPipeline>()
        .init_resource::<FontCx>()
        .init_resource::<RemSize>()
        .init_resource::<ScaleCx>()
        .init_resource::<Assets<Image>>()
        .init_resource::<TileFonts>()
        .init_resource::<crate::pad::PadGlyphs>()
        .init_resource::<RuntimeImages>()
        .insert_resource(ScriptState::over_no_plugins())
        .insert_resource(Tree(ui, menu));
    app.add_systems(Update, paint_tree);
    app.add_systems(PostUpdate, (ApplyDeferred, propagate_ui_target_cameras, sync_font_size_to_em_size, ui_layout_system).chain());
    app.configure_sets(PostUpdate, PropagateSet::<ComputedUiTargetCamera>::default().after(propagate_ui_target_cameras).before(ui_layout_system));
    app.configure_sets(PostUpdate, PropagateSet::<ComputedUiRenderTargetInfo>::default().after(propagate_ui_target_cameras).before(ui_layout_system));
    let camera = app
        .world_mut()
        .spawn((Camera2d, Camera { computed: ComputedCameraValues { target_info: Some(RenderTargetInfo { physical_size: VIEW, scale_factor: SCALE }), ..default() }, ..default() }))
        .id();
    app.insert_resource(Canvas(camera));
    app
}

/// A real texture of `w` x `h` texels, given to the runtime name `name`.
fn give(app: &mut App, name: &str, w: u32, h: u32) -> Handle<Image> {
    let image = Image::new_fill(Extent3d { width: w, height: h, depth_or_array_layers: 1 }, TextureDimension::D2, &[255, 255, 255, 255], TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
    let handle = app.world_mut().resource_mut::<Assets<Image>>().add(image);
    app.world_mut().resource_mut::<RuntimeImages>().set(name, handle.clone(), Vec2::new(w as f32, h as f32));
    handle
}

/// A frame painted and laid out (target cameras propagate a frame late on the first).
fn frame(app: &mut App) {
    app.update();
    app.update();
}

fn set(app: &mut App, path: &str, name: &str, v: f64) {
    let mut tree = app.world_mut().resource_mut::<Tree>();
    let t = tree.0.find_tile(path).unwrap();
    tree.0.set_trait(t, name, Value::Num(v));
    tree.0.update();
}

/// The one drawn picture node.
fn picture(app: &mut App) -> Entity {
    let world = app.world_mut();
    let found: Vec<Entity> = world.query_filtered::<Entity, With<ImageNode>>().iter(world).collect();
    assert_eq!(found.len(), 1, "one picture drawn");
    found[0]
}

/// Where a point of a node, in interface units from its top left, stands on the image, in
/// pixels, as Bevy laid it out.
fn on_image(world: &World, node: Entity, at: (f64, f64)) -> (f64, f64) {
    let size = world.get::<ComputedNode>(node).unwrap().size();
    let local = Vec2::new((at.0 * PX) as f32, (at.1 * PX) as f32) - size / 2.0;
    let p = world.get::<UiGlobalTransform>(node).unwrap().transform_point2(local);
    (f64::from(p.x), f64::from(p.y))
}

fn near(got: (f64, f64), want: (f64, f64)) {
    assert!((got.0 - want.0).abs() < 1e-3 && (got.1 - want.1).abs() < 1e-3, "expected {want:?} px, got {got:?}");
}

/// The tile's box and turn as the layout has them, and where `at` (from the box's top left)
/// should stand once turned, in pixels.
fn wanted(app: &mut App, path: &str, at: (f64, f64)) -> (f64, f64) {
    let tree = app.world().resource::<Tree>();
    let t = tree.0.find_tile(path).unwrap();
    let item = layout::layout(&tree.0, t, &|_, _, _| (0.0, 0.0), true).remove(0);
    let point = (item.rect.left + at.0, item.rect.top + at.1);
    let (x, y) = item.turn.map_or(point, |turn| turn.point(item.rect, point));
    (x * PX, y * PX)
}

const TURNED: &str = r#"<menu name="T"><image name="P"><filename>nvf:p</filename><x>10.3</x><y>20.7</y><width>20</width><height>10</height><zoom>-1</zoom>
    <rotateangle>0</rotateangle><rotateaxisx>3</rotateaxisx><rotateaxisy>7</rotateaxisy></image></menu>"#;

/// An off-middle pivot at 0.75 pixels a unit: the pivot stays where the unturned tile has
/// it and the far corner lands where the turn puts it, to a thousandth of a pixel (a
/// rounded 15 x 8 node would put them a quarter pixel off); one node from unturned through
/// two angles and a new pivot back to unturned.
#[test]
fn a_turned_picture_stands_about_its_pivot_at_a_fractional_scale_and_keeps_its_node() {
    let mut app = app(TURNED);
    give(&mut app, "p", 4, 4);
    frame(&mut app);
    let node = picture(&mut app);
    assert!(app.world().get::<LayoutConfig>(node).is_none(), "unturned: rounded as before");
    assert_eq!(*app.world().get::<UiTransform>(node).unwrap(), UiTransform::IDENTITY);
    for (angle, pivot) in [(0.5, (3.0, 7.0)), (-2.25, (3.0, 7.0)), (-2.25, (17.0, 1.5))] {
        set(&mut app, "T/P", "rotateangle", angle);
        set(&mut app, "T/P", "rotateaxisx", pivot.0);
        set(&mut app, "T/P", "rotateaxisy", pivot.1);
        frame(&mut app);
        assert_eq!(picture(&mut app), node, "turned in place at {angle}, pivot {pivot:?}");
        assert_eq!(app.world().get::<LayoutConfig>(node), Some(&UNROUNDED));
        for at in [pivot, (20.0, 10.0), (0.0, 0.0)] {
            let want = wanted(&mut app, "T/P", at);
            near(on_image(app.world(), node, at), want);
        }
        near(on_image(app.world(), node, pivot), ((10.3 + pivot.0) * PX, (20.7 + pivot.1) * PX));
    }
    set(&mut app, "T/P", "rotateangle", 0.0);
    frame(&mut app);
    assert_eq!(picture(&mut app), node, "turned back in place");
    assert_eq!(*app.world().get::<UiTransform>(node).unwrap(), UiTransform::IDENTITY);
    assert!(app.world().get::<LayoutConfig>(node).is_none(), "rounded again");
}

/// A runtime picture at its texels' size (no `zoom`) takes its new size when its entry is
/// given a texture of another size under the same name.
#[test]
fn a_resized_runtime_picture_stands_at_its_new_size() {
    let mut app = app(r#"<menu name="T"><image name="P"><filename>nvf:outline</filename><width>300</width><height>200</height></image></menu>"#);
    let first = give(&mut app, "outline", 64, 32);
    frame(&mut app);
    let node = picture(&mut app);
    assert_eq!(app.world().get::<ImageNode>(node).unwrap().image, first);
    assert_eq!(app.world().get::<ComputedNode>(node).unwrap().size(), Vec2::new(48.0, 24.0));
    let second = give(&mut app, "outline", 128, 16);
    frame(&mut app);
    let node = picture(&mut app);
    assert_eq!(app.world().get::<ImageNode>(node).unwrap().image, second);
    assert_eq!(app.world().get::<ComputedNode>(node).unwrap().size(), Vec2::new(96.0, 12.0));
}

/// A `tile` runtime picture is repeated by Bevy's tiles over its box: after a resize the
/// node draws the new texture and the repeats stand its texels times the zoom apart in
/// interface units (16 x 4 texels at zoom 50: 8 x 2 units, 6 x 1.5 pixels), as Bevy works the
/// repeats out from the node's logical size over the texture's own size times the stretch
/// (`bevy_ui_render`'s `compute_tiled_axis`; the size given to [`RuntimeImages::set`] is the
/// texture's); the box stays the tile's.
#[test]
fn a_resized_tiled_runtime_picture_repeats_its_new_texture() {
    let mut app = app(r#"<menu name="T"><image name="P"><filename>nvf:ticks</filename><width>80</width><height>40</height><tile>&true;</tile><zoom>50</zoom></image></menu>"#);
    give(&mut app, "ticks", 8, 8);
    frame(&mut app);
    let second = give(&mut app, "ticks", 16, 4);
    frame(&mut app);
    let node = picture(&mut app);
    let image = app.world().get::<ImageNode>(node).unwrap();
    assert_eq!(image.image, second);
    assert_eq!(image.image_mode, NodeImageMode::Tiled { tile_x: true, tile_y: true, stretch_value: 0.5 });
    let NodeImageMode::Tiled { stretch_value, .. } = image.image_mode else { unreachable!() };
    let texels = app.world().resource::<Assets<Image>>().get(&second).unwrap().size().as_vec2();
    assert_eq!(texels, Vec2::new(16.0, 4.0));
    let computed = app.world().get::<ComputedNode>(node).unwrap();
    assert_eq!(computed.size(), Vec2::new(60.0, 30.0));
    // Bevy's count of repeats over the node, and so how far apart they stand in pixels.
    let repeats = computed.size() * computed.inverse_scale_factor() / (texels * stretch_value);
    let apart = computed.size() / repeats;
    assert!((apart - Vec2::new(6.0, 1.5)).abs().max_element() < 1e-4, "repeats {apart} pixels apart");
}

/// A turned picture in a clip window at fractional places, the window moved across pixel
/// edges: the pivot and the corners stay where the turn puts them (the window laid out
/// unrounded with it); turned back, the window and the picture are rounded again.
#[test]
fn a_turned_picture_in_a_moving_fractional_clip_keeps_its_pivot() {
    let mut app = app(
        r#"<menu name="T"><rect name="W"><x>10.3</x><y>5.45</y><width>60</width><height>50</height><clips>&true;</clips>
          <image name="P"><filename>nvf:p</filename><x>4.1</x><y>6.2</y><width>20</width><height>10</height><zoom>-1</zoom>
            <rotateangle>0.7</rotateangle><rotateaxisx>3</rotateaxisx><rotateaxisy>7</rotateaxisy></image></rect></menu>"#,
    );
    give(&mut app, "p", 4, 4);
    frame(&mut app);
    let node = picture(&mut app);
    let wrapper = app.world().get::<ChildOf>(node).unwrap().parent();
    for x in [10.3, 11.17, 12.04, 13.5] {
        set(&mut app, "T/W", "x", x);
        frame(&mut app);
        assert_eq!(picture(&mut app), node, "moved in place to {x}");
        assert_eq!(app.world().get::<LayoutConfig>(wrapper), Some(&UNROUNDED), "the window unrounded with its turned picture");
        for at in [(3.0, 7.0), (20.0, 10.0), (0.0, 0.0)] {
            let want = wanted(&mut app, "T/W/P", at);
            near(on_image(app.world(), node, at), want);
        }
        near(on_image(app.world(), node, (3.0, 7.0)), ((x + 4.1 + 3.0) * PX, (5.45 + 6.2 + 7.0) * PX));
    }
    set(&mut app, "T/W/P", "rotateangle", 0.0);
    frame(&mut app);
    assert_eq!(picture(&mut app), node);
    assert!(app.world().get::<LayoutConfig>(wrapper).is_none() && app.world().get::<LayoutConfig>(node).is_none(), "rounded again");
}
