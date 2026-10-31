//! Headless layout regression coverage for bitmap glyph lines (#1221).

use crate::interface::{set_marked, u, Face};
use bevy::app::{HierarchyPropagatePlugin, PostUpdate, PropagateSet, TaskPoolPlugin};
use bevy::camera::{Camera, Camera2d, ComputedCameraValues, RenderTargetInfo};
use bevy::image::Image;
use bevy::prelude::*;
use bevy::text::{FontCx, RemSize, ScaleCx, TextPipeline};
use bevy::ui::ui_surface::UiSurface;
use bevy::ui::update::propagate_ui_target_cameras;
use bevy::ui::{
    sync_font_size_to_em_size, ui_layout_system, ComputedNode, ComputedUiRenderTargetInfo,
    ComputedUiTargetCamera, UiGlobalTransform, UiScale, UiTargetCamera,
};
use fnv_font::{Font, Glyph, Line, Placed};

const VIEW_W: u32 = 1280;
const ANCESTOR_AT: f32 = 11.6;
const BASE: f32 = 455.3;
const U_WIDTH: f32 = 13.0;
const U_HEIGHT: f32 = 15.0;
const U_ASCENT: f32 = 12.0;
const X_ASCENT: f32 = 14.0;

#[derive(Component, Clone)]
struct Ink;

/// A minimal real Bevy UI layout app: no window, renderer, picking, or text rendering.
/// This follows Bevy's own `bevy_ui::layout` tests, retaining only the systems needed
/// to propagate a target camera and calculate `ComputedNode` / `UiGlobalTransform`.
fn app(height: u32) -> (App, Entity) {
    let mut app = App::new();
    app.add_plugins(TaskPoolPlugin::default());
    app.add_plugins(HierarchyPropagatePlugin::<ComputedUiTargetCamera>::new(
        PostUpdate,
    ));
    app.add_plugins(HierarchyPropagatePlugin::<ComputedUiRenderTargetInfo>::new(
        PostUpdate,
    ));
    app.init_resource::<UiScale>();
    app.init_resource::<UiSurface>();
    app.init_resource::<TextPipeline>();
    app.init_resource::<FontCx>();
    app.init_resource::<RemSize>();
    app.init_resource::<ScaleCx>();
    app.add_systems(
        PostUpdate,
        (
            ApplyDeferred,
            propagate_ui_target_cameras,
            sync_font_size_to_em_size,
            ui_layout_system,
        )
            .chain(),
    );
    app.configure_sets(
        PostUpdate,
        PropagateSet::<ComputedUiTargetCamera>::default()
            .after(propagate_ui_target_cameras)
            .before(ui_layout_system),
    );
    app.configure_sets(
        PostUpdate,
        PropagateSet::<ComputedUiRenderTargetInfo>::default()
            .after(propagate_ui_target_cameras)
            .before(ui_layout_system),
    );
    let camera = app
        .world_mut()
        .spawn((
            Camera2d,
            Camera {
                computed: ComputedCameraValues {
                    target_info: Some(RenderTargetInfo {
                        physical_size: UVec2::new(VIEW_W, height),
                        scale_factor: 1.0,
                    }),
                    ..default()
                },
                ..default()
            },
        ))
        .id();
    (app, camera)
}

fn face(image: Handle<Image>) -> Face {
    let mut glyphs = vec![Glyph::default(); 256];
    glyphs[b'u' as usize] = Glyph {
        uv: [0.0, 0.0, U_WIDTH / 64.0, U_HEIGHT / 64.0],
        width: U_WIDTH,
        height: U_HEIGHT,
        ascent: U_ASCENT,
        ..default()
    };
    // A differently shaped glyph proves that the shared baseline comes from the line
    // anchor, not from every glyph coincidentally having the same top.
    glyphs[b'x' as usize] = Glyph {
        uv: [U_WIDTH / 64.0, 0.0, 2.0 * U_WIDTH / 64.0, 19.0 / 64.0],
        width: U_WIDTH,
        height: 19.0,
        ascent: X_ASCENT,
        ..default()
    };
    Face {
        font: Font {
            size: 20.0,
            texture: String::new(),
            glyphs,
        },
        image: Some((image, Vec2::splat(64.0))),
    }
}

fn line() -> Line {
    Line {
        // The second u begins 13 interface units later. At 720 px high that is
        // 9.75 physical pixels: the two copies are deliberately in different x phases.
        glyphs: vec![
            Placed { code: b'u', x: 0.0 },
            Placed {
                code: b'u',
                x: U_WIDTH,
            },
            Placed {
                code: b'x',
                x: 2.0 * U_WIDTH,
            },
        ],
        width: 3.0 * U_WIDTH,
    }
}

fn draw(app: &mut App, camera: Entity) -> Entity {
    // Explicit glyph dimensions mean this layout test needs no asset server or renderer.
    let face = face(Handle::<Image>::default());
    let root = {
        let mut commands = app.world_mut().commands();
        commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: u(ANCESTOR_AT),
                    top: u(ANCESTOR_AT),
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                UiTargetCamera(camera),
            ))
            .with_children(|p| set_marked(p, &face, &line(), 0.0, BASE, Color::WHITE, Ink))
            .id()
    };
    app.world_mut().flush();
    root
}

fn update_layout(app: &mut App) {
    // Target-camera propagation is deferred before the layout pass on the first update.
    app.update();
    app.update();
}

fn y_of(world: &World, entity: Entity) -> f32 {
    world
        .get::<UiGlobalTransform>(entity)
        .unwrap()
        .to_scale_angle_translation()
        .2
        .y
}

fn x_of(world: &World, entity: Entity) -> f32 {
    world
        .get::<UiGlobalTransform>(entity)
        .unwrap()
        .to_scale_angle_translation()
        .2
        .x
}

fn top_of(world: &World, entity: Entity) -> f32 {
    y_of(world, entity) - world.get::<ComputedNode>(entity).unwrap().size().y / 2.0
}

fn children(world: &World, entity: Entity) -> Vec<Entity> {
    world
        .entity(entity)
        .get::<Children>()
        .unwrap()
        .iter()
        .collect()
}

fn near(a: f32, b: f32) {
    assert!((a - b).abs() < 0.001, "expected {b}, got {a}");
}

fn geometry(world: &World, root: Entity, scale: f32) -> (Entity, Vec<Entity>, Vec<Vec2>, Vec<f32>) {
    let anchors = children(world, root);
    assert_eq!(anchors.len(), 1, "one line owns all of its glyphs");
    let anchor = anchors[0];
    let glyphs = children(world, anchor);
    assert_eq!(glyphs.len(), 3);
    assert!(
        world.get::<Ink>(anchor).is_none(),
        "the layout anchor is not a coloured glyph"
    );
    assert!(glyphs
        .iter()
        .all(|&glyph| world.get::<Ink>(glyph).is_some()));

    let sizes: Vec<Vec2> = glyphs
        .iter()
        .map(|&glyph| world.get::<ComputedNode>(glyph).unwrap().size())
        .collect();
    let bases = [U_ASCENT, U_ASCENT, X_ASCENT]
        .into_iter()
        .zip(&glyphs)
        .map(|(ascent, &glyph)| top_of(world, glyph) + ascent * scale)
        .collect();
    (anchor, glyphs, sizes, bases)
}

#[test]
fn bitmap_glyph_line_keeps_fractional_quads_on_one_pixel_baseline_after_resize() {
    let (mut app, camera) = app(720);
    let root = draw(&mut app, camera);
    update_layout(&mut app);

    let world = app.world();
    let (anchor, glyphs, sizes, bases) = geometry(world, root, 0.75);
    let ancestor_top = top_of(world, root);
    near(y_of(world, anchor), ancestor_top + (BASE * 0.75).round());
    near(y_of(world, anchor), y_of(world, anchor).round());
    near(sizes[0].x, 9.75);
    near(sizes[0].y, 11.25);
    near(sizes[1].x, sizes[0].x);
    near(sizes[1].y, sizes[0].y);
    near(top_of(world, glyphs[2]) + X_ASCENT * 0.75, bases[0]);
    near(bases[0], bases[1]);
    near(bases[1], bases[2]);
    near(bases[0], y_of(world, anchor));
    // The repeated u quads occupy different fractional horizontal phases but retain the
    // same unrounded physical size and top edge.
    near(top_of(world, glyphs[0]), top_of(world, glyphs[1]));
    near(x_of(world, glyphs[1]) - x_of(world, glyphs[0]), 9.75);
    assert!(
        (x_of(world, glyphs[0]).fract() - x_of(world, glyphs[1]).fract()).abs() > 0.1,
        "the two glyphs exercise different x phases"
    );

    {
        let world = app.world_mut();
        let mut camera_entity = world.entity_mut(camera);
        let mut camera_node = camera_entity.get_mut::<Camera>().unwrap();
        camera_node
            .computed
            .target_info
            .as_mut()
            .unwrap()
            .physical_size = UVec2::new(VIEW_W, 1080);
    }
    update_layout(&mut app);

    let world = app.world();
    let (anchor, glyphs, sizes, bases) = geometry(world, root, 1.125);
    let ancestor_top = top_of(world, root);
    near(y_of(world, anchor), ancestor_top + (BASE * 1.125).round());
    near(y_of(world, anchor), y_of(world, anchor).round());
    near(sizes[0].x, 14.625);
    near(sizes[0].y, 16.875);
    near(sizes[1].x, sizes[0].x);
    near(sizes[1].y, sizes[0].y);
    near(top_of(world, glyphs[1]), top_of(world, glyphs[0]));
    near(bases[0], bases[1]);
    near(bases[1], bases[2]);
    near(bases[0], y_of(world, anchor));
    near(x_of(world, glyphs[1]) - x_of(world, glyphs[0]), 14.625);
}
