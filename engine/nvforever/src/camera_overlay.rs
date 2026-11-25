//! JohnnyGuitar NVSE 3D camera overlay renderer (`JG_CameraOverlayRef_PreIS` and
//! `JG_CameraOverlayRef_PreUI`; `docs/spec-notes/ext-jg-nif-camera.md`).
//!
//! - **Vanilla fast-path [design]:** when no script has watched either built-in camera
//!   overlay reference (`CAMERA_OVERLAY_PRE_IS` / `CAMERA_OVERLAY_PRE_UI`), this module
//!   spawns no entities, cameras, render targets or UI nodes (`O(1)` set check per frame).
//! - **`PreIS` (`OVERLAY_PRE_IS_LAYER`) [design]:** a camera-attached holder under the main
//!   `WorldCamera` rotated by `world::game_to_bevy()` (so game `+Y` looks straight ahead
//!   along Bevy `-Z`, `+X` right, `+Z` up). The main camera renders `OVERLAY_PRE_IS_LAYER`
//!   before tonemapping and `ImageSpaceGrade`; sun/reflection/scope/extra cameras exclude it.
//! - **`PreUI` (`OVERLAY_PRE_UI_LAYER`) [design]:** a lazily spawned offscreen `Camera3d`
//!   (`order: -5`, transparent clear) synced to the main camera's perspective FOV/aspect,
//!   composited full-screen via `ImageNode` at `GlobalZIndex(PRE_UI_Z_INDEX)` (`-2`, after
//!   `ImageSpaceGrade` and strictly under HUD/UI tiles and loading screens). The offscreen
//!   camera stays inactive whenever `PreUI` has no attached children or a loading screen is up.
//! - **Loading screen suppression [documented: JohnnyGuitar NVSE 5.28 release notes,
//!   "Don't render camera overlays during loading screen"]:** while `Loading::active()` is
//!   true, both stages are hidden and the `PreUI` offscreen camera is deactivated.

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ImageRenderTarget, RenderTarget};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::AmbientLight;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::window::PrimaryWindow;
use fnv_game::nvse_jg_nif::{CAMERA_OVERLAY_PRE_IS, CAMERA_OVERLAY_PRE_UI};
use nvf_play::loading::Loading;
use nvf_play::script::ScriptState;
use nvf_play::RefVisual;
use std::sync::Arc;

/// Render layer of 3D camera overlays drawn before image-space post-processing
/// (`JG_CameraOverlayRef_PreIS`) [design].
pub const OVERLAY_PRE_IS_LAYER: usize = 18;

/// Render layer of 3D camera overlays drawn after image-space post-processing and
/// before HUD/UI (`JG_CameraOverlayRef_PreUI`) [design].
pub const OVERLAY_PRE_UI_LAYER: usize = 19;

/// UI Z-index of the `PreUI` full-screen composite (`ImageNode`), strictly under
/// HUD/menu tiles (`>= 0`) and under the 2D scope mask (`-1`) [design].
pub const PRE_UI_Z_INDEX: i32 = -2;

/// Near/far clip planes of the `PreUI` offscreen overlay camera [design: allows close
/// camera-space overlay planes placed nearer than the world camera's 5.0 near plane].
const PRE_UI_NEAR: f32 = 0.1;
const PRE_UI_FAR: f32 = 10_000.0;

/// Which JohnnyGuitar camera overlay stage a root belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OverlayStage {
    PreIs,
    PreUi,
}

/// Marker on a spawned camera overlay holder (`RefVisual(CAMERA_OVERLAY_*)`).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CameraOverlayRoot {
    pub stage: OverlayStage,
}

/// Marker on the built-in `"Scene Root"` child under a [`CameraOverlayRoot`].
#[derive(Component, Debug, Clone, Copy)]
pub struct CameraOverlaySceneRoot;

/// Marker on the lazily spawned offscreen `Camera3d` rendering [`OVERLAY_PRE_UI_LAYER`].
#[derive(Component, Debug, Clone, Copy)]
pub struct CameraOverlayPreUiCamera;

/// Marker on the full-screen `ImageNode` compositing [`OVERLAY_PRE_UI_LAYER`] under HUD/UI.
#[derive(Component, Debug, Clone, Copy)]
pub struct CameraOverlayPreUiNode;

struct PreIsStage {
    camera: Entity,
    root: Entity,
    scene_root: Entity,
}

struct PreUiTarget {
    image: Handle<Image>,
    ui_node: Entity,
    size: UVec2,
}

struct PreUiStage {
    camera: Entity,
    root: Entity,
    scene_root: Entity,
    target: Option<PreUiTarget>,
}

/// Runtime state of the camera overlay roots and lazy `PreUI` offscreen pass.
#[derive(Resource, Default)]
pub(crate) struct CameraOverlayState {
    session: Option<Arc<()>>,
    pre_is: Option<PreIsStage>,
    pre_ui: Option<PreUiStage>,
}

impl CameraOverlayState {
    fn is_empty(&self) -> bool {
        self.pre_is.is_none() && self.pre_ui.is_none()
    }
}

pub(crate) fn install(app: &mut App) {
    app.init_resource::<CameraOverlayState>();
}

fn new_target_image(images: &mut Assets<Image>, size: UVec2) -> Handle<Image> {
    let mut image = Image::new_fill(
        Extent3d {
            width: size.x.max(1),
            height: size.y.max(1),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Bgra8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    images.add(image)
}

fn overlay_projection(main: Option<&Projection>) -> Projection {
    let (fov, aspect_ratio) = match main {
        Some(Projection::Perspective(p)) => (p.fov, p.aspect_ratio),
        _ => (PerspectiveProjection::default().fov, PerspectiveProjection::default().aspect_ratio),
    };
    Projection::Perspective(PerspectiveProjection {
        fov,
        aspect_ratio,
        near: PRE_UI_NEAR,
        far: PRE_UI_FAR,
        ..default()
    })
}

fn teardown_pre_is(commands: &mut Commands, stage: PreIsStage) {
    commands.entity(stage.root).try_despawn();
}

fn teardown_pre_ui(commands: &mut Commands, stage: PreUiStage, images: Option<&mut Assets<Image>>) {
    if let Some(target) = stage.target {
        commands.entity(target.ui_node).try_despawn();
        if let Some(images) = images {
            images.remove(&target.image);
        }
    }
    commands.entity(stage.camera).try_despawn();
}

fn teardown_all(
    commands: &mut Commands,
    state: &mut CameraOverlayState,
    images: Option<&mut Assets<Image>>,
) {
    if let Some(stage) = state.pre_is.take() {
        teardown_pre_is(commands, stage);
    }
    if let Some(stage) = state.pre_ui.take() {
        teardown_pre_ui(commands, stage, images);
    }
}

pub(crate) type PreUiCams<'w, 's> = Query<
    'w,
    's,
    (&'static mut Camera, &'static mut Projection),
    (With<CameraOverlayPreUiCamera>, Without<crate::fly::FlyCam>),
>;

pub(crate) type OverlayNodes<'w, 's> = Query<
    'w,
    's,
    (&'static mut Visibility, Option<&'static Children>),
    (Without<CameraOverlayPreUiNode>, Without<CameraOverlayPreUiCamera>),
>;

pub(crate) type PreUiUiNodes<'w, 's> = Query<
    'w,
    's,
    (&'static mut Visibility, &'static mut ImageNode),
    With<CameraOverlayPreUiNode>,
>;

#[allow(clippy::too_many_arguments)]
pub(crate) fn sync(
    mut script: Option<ResMut<ScriptState>>,
    mut state: ResMut<CameraOverlayState>,
    commands: Commands,
    main_cam: Query<(Entity, &Projection), crate::fly::WorldCamera>,
    pre_ui_cams: PreUiCams,
    overlay_nodes: OverlayNodes,
    pre_ui_ui: PreUiUiNodes,
    window: Query<&Window, With<PrimaryWindow>>,
    images: Option<ResMut<Assets<Image>>>,
    loading: Option<Res<Loading>>,
) {
    let game = script
        .as_deref_mut()
        .and_then(|s| s.rt.as_mut())
        .map(|rt| &rt.game);
    sync_game(
        game,
        &mut state,
        commands,
        main_cam,
        pre_ui_cams,
        overlay_nodes,
        pre_ui_ui,
        window,
        images,
        loading.as_deref().is_some_and(Loading::active),
    );
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn sync_game(
    game: Option<&fnv_game::Game<'_>>,
    state: &mut CameraOverlayState,
    mut commands: Commands,
    main_cam: Query<(Entity, &Projection), crate::fly::WorldCamera>,
    mut pre_ui_cams: PreUiCams,
    mut overlay_nodes: OverlayNodes,
    mut pre_ui_ui: PreUiUiNodes,
    window: Query<&Window, With<PrimaryWindow>>,
    mut images: Option<ResMut<Assets<Image>>>,
    loading_active: bool,
) {
    let Some(game) = game else {
        if !state.is_empty() || state.session.is_some() {
            teardown_all(&mut commands, state, images.as_deref_mut());
            state.session = None;
        }
        return;
    };

    let session = game.scene.attachment_session();
    if state
        .session
        .as_ref()
        .is_none_or(|old| !Arc::ptr_eq(old, &session))
    {
        teardown_all(&mut commands, state, images.as_deref_mut());
        state.session = Some(session);
    }

    // O(1) vanilla empty fast-path: do nothing until a script actually touches an overlay ref.
    let want_pre_is = game.scene.is_watched(CAMERA_OVERLAY_PRE_IS);
    let want_pre_ui = game.scene.is_watched(CAMERA_OVERLAY_PRE_UI);
    if !want_pre_is && !want_pre_ui && state.is_empty() {
        return;
    }

    let main = main_cam.iter().next();

    // ---- Stage 1: JG_CameraOverlayRef_PreIS (drawn by main camera before ImageSpaceGrade) ----
    if want_pre_is {
        let desired_vis = if loading_active {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        let alive = state.pre_is.as_ref().is_some_and(|s| {
            main.is_some_and(|(cam, _)| cam == s.camera)
                && overlay_nodes.get(s.root).is_ok()
                && overlay_nodes.get(s.scene_root).is_ok()
        });
        if !alive {
            if let Some(old) = state.pre_is.take() {
                teardown_pre_is(&mut commands, old);
            }
            if let Some((cam_entity, _)) = main {
                let layer = RenderLayers::layer(OVERLAY_PRE_IS_LAYER);
                let root = commands
                    .spawn((
                        Name::new("JG Camera Overlay PreIS"),
                        CameraOverlayRoot {
                            stage: OverlayStage::PreIs,
                        },
                        RefVisual(CAMERA_OVERLAY_PRE_IS.0),
                        crate::world::game_to_bevy(),
                        desired_vis,
                        layer.clone(),
                        ChildOf(cam_entity),
                    ))
                    .id();
                let scene_root = commands
                    .spawn((
                        Name::new("Scene Root"),
                        CameraOverlaySceneRoot,
                        Transform::IDENTITY,
                        Visibility::Inherited,
                        layer,
                        ChildOf(root),
                    ))
                    .id();
                state.pre_is = Some(PreIsStage {
                    camera: cam_entity,
                    root,
                    scene_root,
                });
            }
        } else if let Some(stage) = &state.pre_is {
            if let Ok((mut vis, _)) = overlay_nodes.get_mut(stage.root) {
                if *vis != desired_vis {
                    *vis = desired_vis;
                }
            }
        }
    } else if let Some(old) = state.pre_is.take() {
        teardown_pre_is(&mut commands, old);
    }

    // ---- Stage 2: JG_CameraOverlayRef_PreUI (offscreen Camera3d composited under HUD/UI) ----
    if want_pre_ui {
        let proj = overlay_projection(main.map(|(_, p)| p));
        let size = window
            .iter()
            .next()
            .map(|w| UVec2::new(w.physical_width().max(1), w.physical_height().max(1)))
            .unwrap_or(UVec2::new(1280, 720));

        let alive = state.pre_ui.as_ref().is_some_and(|s| {
            pre_ui_cams.get(s.camera).is_ok()
                && overlay_nodes.get(s.root).is_ok()
                && overlay_nodes.get(s.scene_root).is_ok()
        });
        if !alive {
            if let Some(old) = state.pre_ui.take() {
                teardown_pre_ui(&mut commands, old, images.as_deref_mut());
            }
            let layer = RenderLayers::layer(OVERLAY_PRE_UI_LAYER);
            let mut cam_cmd = commands.spawn((
                Name::new("JG Camera Overlay PreUI Camera"),
                CameraOverlayPreUiCamera,
                Camera3d::default(),
                Camera {
                    order: -5,
                    is_active: false,
                    clear_color: ClearColorConfig::Custom(Color::NONE),
                    ..default()
                },
                proj.clone(),
                Tonemapping::Linear,
                Msaa::Off,
                AmbientLight {
                    color: Color::WHITE,
                    brightness: 500.0,
                    ..default()
                },
                Transform::IDENTITY,
                layer.clone(),
            ));
            let target = images.as_deref_mut().map(|imgs| {
                let image = new_target_image(imgs, size);
                cam_cmd.insert(RenderTarget::Image(ImageRenderTarget::from(image.clone())));
                image
            });
            let camera = cam_cmd.id();
            let ui_node = target.map(|image| {
                let node = commands
                    .spawn((
                        Name::new("JG Camera Overlay PreUI Picture"),
                        CameraOverlayPreUiNode,
                        Node {
                            position_type: PositionType::Absolute,
                            width: Val::Percent(100.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        ImageNode::new(image.clone()),
                        GlobalZIndex(PRE_UI_Z_INDEX),
                        Pickable::IGNORE,
                        Visibility::Hidden,
                    ))
                    .id();
                PreUiTarget {
                    image,
                    ui_node: node,
                    size,
                }
            });
            let root = commands
                .spawn((
                    Name::new("JG Camera Overlay PreUI"),
                    CameraOverlayRoot {
                        stage: OverlayStage::PreUi,
                    },
                    RefVisual(CAMERA_OVERLAY_PRE_UI.0),
                    crate::world::game_to_bevy(),
                    Visibility::Hidden,
                    layer.clone(),
                    ChildOf(camera),
                ))
                .id();
            let scene_root = commands
                .spawn((
                    Name::new("Scene Root"),
                    CameraOverlaySceneRoot,
                    Transform::IDENTITY,
                    Visibility::Inherited,
                    layer,
                    ChildOf(root),
                ))
                .id();
            state.pre_ui = Some(PreUiStage {
                camera,
                root,
                scene_root,
                target: ui_node,
            });
        } else if let Some(stage) = state.pre_ui.as_mut() {
            // PreUI only activates its offscreen camera and composite node once something
            // is attached under its root or "Scene Root" (and no loading screen is up).
            let root_extra_children = overlay_nodes
                .get(stage.root)
                .ok()
                .and_then(|(_, ch)| ch)
                .is_some_and(|ch| ch.iter().any(|c| c != stage.scene_root));
            let scene_children = overlay_nodes
                .get(stage.scene_root)
                .ok()
                .and_then(|(_, ch)| ch)
                .is_some_and(|ch| !ch.is_empty());
            let active = !loading_active && (root_extra_children || scene_children);
            let desired_vis = if active {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };

            if let Ok((mut vis, _)) = overlay_nodes.get_mut(stage.root) {
                if *vis != desired_vis {
                    *vis = desired_vis;
                }
            }
            if let Ok((mut cam, mut cam_proj)) = pre_ui_cams.get_mut(stage.camera) {
                if cam.is_active != active {
                    cam.is_active = active;
                }
                if let (Projection::Perspective(cur), Projection::Perspective(want)) =
                    (&*cam_proj, &proj)
                {
                    if (cur.fov - want.fov).abs() > 1e-5
                        || (cur.aspect_ratio - want.aspect_ratio).abs() > 1e-5
                    {
                        *cam_proj = proj.clone();
                    }
                }
            }

            if let Some(imgs) = images.as_deref_mut() {
                let need_resize = stage
                    .target
                    .as_ref()
                    .is_none_or(|t| t.size != size || pre_ui_ui.get(t.ui_node).is_err());
                if need_resize {
                    let old_target = stage.target.take();
                    let new_image = new_target_image(imgs, size);
                    commands
                        .entity(stage.camera)
                        .insert(RenderTarget::Image(ImageRenderTarget::from(
                            new_image.clone(),
                        )));
                    let ui_node = if let Some(old) = old_target {
                        imgs.remove(&old.image);
                        if let Ok((mut vis, mut img_node)) = pre_ui_ui.get_mut(old.ui_node) {
                            img_node.image = new_image.clone();
                            *vis = desired_vis;
                            old.ui_node
                        } else {
                            commands
                                .spawn((
                                    Name::new("JG Camera Overlay PreUI Picture"),
                                    CameraOverlayPreUiNode,
                                    Node {
                                        position_type: PositionType::Absolute,
                                        width: Val::Percent(100.0),
                                        height: Val::Percent(100.0),
                                        ..default()
                                    },
                                    ImageNode::new(new_image.clone()),
                                    GlobalZIndex(PRE_UI_Z_INDEX),
                                    Pickable::IGNORE,
                                    desired_vis,
                                ))
                                .id()
                        }
                    } else {
                        commands
                            .spawn((
                                Name::new("JG Camera Overlay PreUI Picture"),
                                CameraOverlayPreUiNode,
                                Node {
                                    position_type: PositionType::Absolute,
                                    width: Val::Percent(100.0),
                                    height: Val::Percent(100.0),
                                    ..default()
                                },
                                ImageNode::new(new_image.clone()),
                                GlobalZIndex(PRE_UI_Z_INDEX),
                                Pickable::IGNORE,
                                desired_vis,
                            ))
                            .id()
                    };
                    stage.target = Some(PreUiTarget {
                        image: new_image,
                        ui_node,
                        size,
                    });
                } else if let Some(target) = &stage.target {
                    if let Ok((mut vis, _)) = pre_ui_ui.get_mut(target.ui_node) {
                        if *vis != desired_vis {
                            *vis = desired_vis;
                        }
                    }
                }
            }
        }
    } else if let Some(old) = state.pre_ui.take() {
        teardown_pre_ui(&mut commands, old, images.as_deref_mut());
    }
}
