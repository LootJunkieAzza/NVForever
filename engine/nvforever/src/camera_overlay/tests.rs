//! Headless end-to-end tests for JohnnyGuitar 3D camera overlays (`PreIS` & `PreUI`),
//! NIF material/alpha/stencil live consumers, attached particle isolation/mutation,
//! loading-screen suppression, and vanilla no-op invariance.
use super::*;
use crate::authored_test_records as common;
use crate::camera_overlay::{
    self, CameraOverlayPreUiCamera, CameraOverlayPreUiNode, CameraOverlayRoot,
    CameraOverlaySceneRoot, CameraOverlayState, OverlayStage, OVERLAY_PRE_IS_LAYER,
    OVERLAY_PRE_UI_LAYER, PRE_UI_Z_INDEX,
};
use crate::nif_attachments::{self, AttachedNode, Owned, State as AttachState};
use bevy::camera::visibility::RenderLayers;
use bevy::window::PrimaryWindow;
use fnv_game::nvse_jg_nif::CAMERA_OVERLAY_PRE_IS;
use fnv_game::{game::RunCtx, Game, GameConfig};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormStore, RefTable};
use nvf_assets::model::{ModelCache, ModelCtx};
use nvf_assets::particles::{Emitter, ParticleSystemDef};
use nvf_assets::texture::TextureCache;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const PLACED_REF: FormId = FormId(0x411);
const IDENTITY: [f32; 9] = [1., 0., 0., 0., 1., 0., 0., 0., 1.];
static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Default)]
struct Bytes(Vec<u8>);
impl Bytes {
    fn u8(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }
    fn u16(&mut self, v: u16) -> &mut Self {
        self.0.extend(v.to_le_bytes());
        self
    }
    fn u32(&mut self, v: u32) -> &mut Self {
        self.0.extend(v.to_le_bytes());
        self
    }
    fn i32(&mut self, v: i32) -> &mut Self {
        self.0.extend(v.to_le_bytes());
        self
    }
    fn f(&mut self, values: &[f32]) -> &mut Self {
        for v in values {
            self.0.extend(v.to_le_bytes());
        }
        self
    }
    fn text(&mut self, s: &str) -> &mut Self {
        self.u32(s.len() as u32);
        self.0.extend(s.as_bytes());
        self
    }
    fn export(&mut self, s: &str) -> &mut Self {
        self.u8(s.len() as u8 + 1);
        self.0.extend(s.as_bytes());
        self.u8(0)
    }
}

fn av(w: &mut Bytes, name: u32, position: [f32; 3], properties: &[i32]) {
    w.u32(name)
        .u32(0)
        .i32(-1)
        .u32(14)
        .f(&position)
        .f(&IDENTITY)
        .f(&[1.]);
    w.u32(properties.len() as u32);
    for &p in properties {
        w.i32(p);
    }
    w.i32(-1);
}

fn node(name: u32, position: [f32; 3], children: &[i32]) -> Vec<u8> {
    let mut w = Bytes::default();
    av(&mut w, name, position, &[]);
    w.u32(children.len() as u32);
    for &c in children {
        w.i32(c);
    }
    w.u32(0);
    w.0
}

fn shape(name: u32, position: [f32; 3], data: i32, properties: &[i32]) -> Vec<u8> {
    let mut w = Bytes::default();
    av(&mut w, name, position, properties);
    w.i32(data).i32(-1).u32(0).i32(-1).u8(0);
    w.0
}

fn geometry() -> Vec<u8> {
    let mut w = Bytes::default();
    w.i32(0).u16(4).u8(0).u8(0).u8(1);
    w.f(&[0., 0., 0., 1., 0., 0., 0., 1., 0., 1., 1., 0.]);
    w.u16(1).u8(0).f(&[0., 0., 0., 4.]).u8(0);
    w.f(&[0., 0., 1., 0., 0., 1., 1., 1.]);
    w.u16(0)
        .i32(-1)
        .u16(2)
        .u16(1)
        .u16(4)
        .u8(1)
        .u16(0)
        .u16(1)
        .u16(2)
        .u16(3);
    w.0
}

fn material(gloss: f32) -> Vec<u8> {
    let mut w = Bytes::default();
    w.u32(u32::MAX).u32(0).i32(-1);
    w.f(&[1., 0., 0., 0., 0., 0., gloss, 1., 1.]);
    w.0
}

fn alpha_prop(flags: u16, threshold: u8) -> Vec<u8> {
    let mut w = Bytes::default();
    w.u32(u32::MAX).u32(0).i32(-1).u16(flags).u8(threshold);
    w.0
}

fn stencil_prop(flags: u16, reference: u32, mask: u32) -> Vec<u8> {
    let mut w = Bytes::default();
    w.u32(u32::MAX).u32(0).i32(-1).u16(flags).u32(reference).u32(mask);
    w.0
}

fn lit_shader() -> Vec<u8> {
    let mut w = Bytes::default();
    w.u32(u32::MAX).u32(0).i32(-1).u16(1);
    w.u32(1).u32(0).u32(0).f(&[1.]);
    w.u32(3).i32(-1).f(&[0.]).u32(0).f(&[4., 1.]);
    w.0
}

fn nif(blocks: &[(&str, Vec<u8>)], strings: &[&str]) -> Vec<u8> {
    let mut types = Vec::new();
    for (name, _) in blocks {
        if !types.contains(name) {
            types.push(*name);
        }
    }
    let mut w = Bytes(b"Gamebryo File Format, Version 20.2.0.7\n".to_vec());
    w.u32(0x1402_0007)
        .u8(1)
        .u32(11)
        .u32(blocks.len() as u32)
        .u32(34);
    w.export("owned")
        .export("overlay")
        .export("synthetic")
        .u16(types.len() as u16);
    for name in &types {
        w.text(name);
    }
    for (name, _) in blocks {
        w.u16(types.iter().position(|n| n == name).unwrap() as u16);
    }
    for (_, bytes) in blocks {
        w.u32(bytes.len() as u32);
    }
    w.u32(strings.len() as u32)
        .u32(strings.iter().map(|s| s.len()).max().unwrap_or(0) as u32);
    for s in strings {
        w.text(s);
    }
    w.u32(0);
    for (_, bytes) in blocks {
        w.0.extend(bytes);
    }
    w.u32(1).i32(0);
    w.0
}

/// Synthetic camera-overlay NIF with NiMaterialProperty, NiAlphaProperty (opaque/0),
/// and NiStencilProperty (`stencil_flags`) on `OverlayShape` under `OverlayNode`.
fn overlay_nif_with_stencil(stencil_flags: u16) -> Vec<u8> {
    nif(
        &[
            ("BSFadeNode", node(0, [0., 0., 0.], &[1])),
            ("NiNode", node(1, [0., 5., 0.], &[2])),
            ("NiTriStrips", shape(2, [0., 0., 0.], 3, &[4, 5, 6, 7])),
            ("NiTriStripsData", geometry()),
            ("NiMaterialProperty", material(25.)),
            ("NiAlphaProperty", alpha_prop(0, 128)),
            ("NiStencilProperty", stencil_prop(stencil_flags, 0, 255)),
            ("BSShaderPPLightingProperty", lit_shader()),
        ],
        &["OverlayRoot", "OverlayNode", "OverlayShape"],
    )
}

fn overlay_nif() -> Vec<u8> {
    overlay_nif_with_stencil(0)
}

struct OverlayFiles(PathBuf);
impl OverlayFiles {
    fn new() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "nvf-camera-overlay-{}-{nonce}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("meshes/overlay")).unwrap();
        std::fs::write(root.join("meshes/overlay/test.nif"), overlay_nif()).unwrap();
        for mode in 0u16..=3 {
            let flags = (mode << 10) | 0x0015;
            std::fs::write(
                root.join(format!("meshes/overlay/stencil_{mode}.nif")),
                overlay_nif_with_stencil(flags),
            )
            .unwrap();
        }
        Self(root)
    }
}
impl Drop for OverlayFiles {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Source {
    data: fnv_game::GameData,
    forms: FormStore,
    refs: RefTable,
}
fn source() -> Source {
    use common::*;
    let placed = record(
        b"REFR",
        0,
        PLACED_REF.0,
        &[
            sub(b"EDID", z("OverlayHostRef")),
            sub(b"NAME", u32s(&[0x301])),
            sub(b"DATA", vec![0; 24]),
        ],
    );
    let cell = [
        record(
            b"CELL",
            0,
            0x900,
            &[sub(b"EDID", z("OverlayTestCell")), sub(b"DATA", vec![1])],
        ),
        group(
            0x900u32.to_le_bytes(),
            6,
            &group(0x900u32.to_le_bytes(), 9, &placed),
        ),
    ]
    .concat();
    let bytes = [
        tes4(),
        top(
            b"STAT",
            &record(b"STAT", 0, 0x301, &[sub(b"EDID", z("OverlayBase"))]),
        ),
        top(
            b"CELL",
            &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell)),
        ),
    ]
    .concat();
    let plugins = [fnv_esm::Plugin::parse(&bytes, "OverlayTest.esm".into()).unwrap()];
    let lo = fnv_esm::LoadOrder::new(&plugins).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = fnv_esm::Schema::load_dir(root.join("fnv")).unwrap();
    let catalogue = root.join("script/functions.json");
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let raw = serde_json::from_slice(&std::fs::read(&catalogue).unwrap()).unwrap();
    let data = fnv_game::GameData::build(
        &lo,
        &schema,
        FunctionTable::load(&catalogue).unwrap(),
        &raw,
        &refs,
    );
    Source { data, forms, refs }
}

#[derive(Resource)]
struct TestGame(Game<'static>);
#[derive(Resource, Default)]
struct TestMemory(Memory);
#[derive(Resource, Default)]
struct TestLoading(bool);

#[allow(clippy::too_many_arguments)]
fn sys_overlay(
    game: Res<TestGame>,
    mut state: ResMut<CameraOverlayState>,
    commands: Commands,
    main_cam: Query<(Entity, &Projection), crate::fly::WorldCamera>,
    pre_ui_cams: camera_overlay::PreUiCams,
    overlay_nodes: camera_overlay::OverlayNodes,
    pre_ui_ui: camera_overlay::PreUiUiNodes,
    window: Query<&Window, With<PrimaryWindow>>,
    images: Option<ResMut<Assets<Image>>>,
    loading: Res<TestLoading>,
) {
    camera_overlay::sync_game(
        Some(&game.0),
        &mut state,
        commands,
        main_cam,
        pre_ui_cams,
        overlay_nodes,
        pre_ui_ui,
        window,
        images,
        loading.0,
    );
}

#[allow(clippy::too_many_arguments)]
fn sys_produce(
    mut game: ResMut<TestGame>,
    mut state: ResMut<AttachState>,
    commands: Commands,
    models: Option<ModelCtx>,
    rigs: Rigs,
    visuals: Visuals,
    nodes: NodeQuery,
    parents: Query<&ChildOf>,
    layers: Query<&RenderLayers>,
    owned: Owned,
) {
    nif_attachments::produce_game(
        Some(&mut game.0),
        &mut state,
        commands,
        models,
        rigs,
        visuals,
        nodes,
        parents,
        layers,
        owned,
    );
}

#[allow(clippy::too_many_arguments)]
fn sys_mirror(
    mut game: ResMut<TestGame>,
    rigs: Rigs,
    visuals: Visuals,
    parents: Query<&ChildOf>,
    nodes: NodeQuery,
    commands: Commands,
    mut memory: ResMut<TestMemory>,
    materials: Materials,
    shapes: Shapes,
    extra: Extra,
    dropped: Dropped,
) {
    mirror_game(
        &mut game.0,
        rigs,
        visuals,
        parents,
        nodes,
        commands,
        &mut memory.0,
        materials,
        shapes,
        extra,
        dropped,
    );
}

fn harness() -> (App, OverlayFiles, Entity, Entity) {
    let files = OverlayFiles::new();
    let src: &'static Source = Box::leak(Box::new(source()));
    let game = Game::new(
        &src.data,
        fnv_sim::World::new(&src.forms, &src.refs, Default::default()),
        GameConfig {
            strict: true,
            ..default()
        },
    );
    let mut app = App::new();
    app.add_plugins(bevy::transform::TransformPlugin);
    let schema = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas/nif");
    let data = GameData::open_for_plugins(&files.0, &[] as &[&str], &schema).unwrap();
    app.insert_resource(data)
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<NifMaterial>>()
        .init_resource::<Assets<Image>>()
        .init_resource::<Assets<bevy::mesh::skinning::SkinnedMeshInverseBindposes>>()
        .init_resource::<TextureCache>()
        .init_resource::<ModelCache>()
        .init_resource::<TestLoading>()
        .init_resource::<crate::particles::Particles>()
        .insert_resource(TestGame(game))
        .init_resource::<TestMemory>();
    camera_overlay::install(&mut app);
    nif_attachments::install(&mut app);
    app.add_systems(
        PostUpdate,
        (sys_overlay, sys_produce, sys_mirror)
            .chain()
            .before(TransformSystems::Propagate),
    );
    let win = app
        .world_mut()
        .spawn((
            Window {
                resolution: bevy::window::WindowResolution::new(1280, 720),
                ..default()
            },
            PrimaryWindow,
        ))
        .id();
    let main_cam = app
        .world_mut()
        .spawn((
            Camera3d::default(),
            Projection::Perspective(PerspectiveProjection {
                fov: 75.0f32.to_radians(),
                aspect_ratio: 16.0 / 9.0,
                near: 5.0,
                far: 100_000.0,
                ..default()
            }),
            Transform::IDENTITY,
            crate::fly::FlyCam {
                yaw: 0.0,
                pitch: 0.0,
                speed: 0.0,
            },
        ))
        .id();
    (app, files, main_cam, win)
}

fn eval(app: &mut App, body: &str) -> XVal {
    let code = format!("Begin Function {{ }}\n{body}\nEnd\n");
    let mut game = app.world_mut().resource_mut::<TestGame>();
    game.0
        .run_query(&code, RunCtx::default())
        .unwrap_or_else(|e| panic!("script failed: {e}\n{body}"))
}

fn num(app: &mut App, body: &str) -> f64 {
    match eval(app, body) {
        XVal::Num(x) => x,
        other => panic!("expected Num, got {other:?}"),
    }
}

#[test]
fn vanilla_fast_path_spawns_zero_overlay_entities_cameras_or_images() {
    let (mut app, _files, _cam, _win) = harness();
    app.update();
    app.update();
    assert_eq!(
        app.world_mut()
            .query::<&CameraOverlayRoot>()
            .iter(app.world())
            .count(),
        0,
        "vanilla session must spawn no camera overlay roots"
    );
    assert_eq!(
        app.world_mut()
            .query::<&CameraOverlayPreUiCamera>()
            .iter(app.world())
            .count(),
        0,
        "vanilla session must spawn no PreUI overlay camera"
    );
    assert_eq!(
        app.world_mut()
            .query::<&CameraOverlayPreUiNode>()
            .iter(app.world())
            .count(),
        0,
        "vanilla session must spawn no PreUI composite UI node"
    );
    assert_eq!(
        app.world().resource::<Assets<Image>>().len(),
        0,
        "vanilla session must allocate no PreUI target images"
    );
}

#[test]
fn pre_is_overlay_spawns_transforms_overrides_materials_and_removes() {
    let (mut app, _files, main_cam, _win) = harness();

    // Attach synthetic NIF onto JG_CameraOverlayRef_PreIS.
    eval(
        &mut app,
        "ref rCam\n\
         let rCam := EditorIDToFormID \"JG_CameraOverlayRef_PreIS\"\n\
         AttachModel rCam 3 \"Scene Root|overlay\\test.nif\"",
    );
    app.update();

    // PreIS holder sits under main camera on OVERLAY_PRE_IS_LAYER with game_to_bevy().
    let (root_e, root, parent, tf, layer) = app
        .world_mut()
        .query::<(
            Entity,
            &CameraOverlayRoot,
            &ChildOf,
            &Transform,
            &RenderLayers,
        )>()
        .single(app.world())
        .unwrap();
    assert_eq!(root.stage, OverlayStage::PreIs);
    assert_eq!(parent.parent(), main_cam);
    assert_eq!(*layer, RenderLayers::layer(OVERLAY_PRE_IS_LAYER));
    // Game +Y maps to Bevy -Z (straight ahead of the camera).
    let ahead = tf.transform_point(Vec3::new(0.0, 7.5, 0.0));
    assert!((ahead - Vec3::new(0.0, 0.0, -7.5)).length() < 1e-4);
    assert_eq!(
        app.world().get::<RefVisual>(root_e).unwrap().0,
        CAMERA_OVERLAY_PRE_IS.0
    );

    // Attached shape sits on OVERLAY_PRE_IS_LAYER with NotShadowCaster.
    let (shape_e, orig_mat) = app
        .world_mut()
        .query::<(Entity, &Name, &MeshMaterial3d<NifMaterial>, &RenderLayers)>()
        .iter(app.world())
        .find(|(_, name, _, _)| name.as_str() == "OverlayShape")
        .map(|(e, _, m, l)| {
            assert_eq!(*l, RenderLayers::layer(OVERLAY_PRE_IS_LAYER));
            (e, m.0.clone())
        })
        .expect("OverlayShape spawned under PreIS");
    assert!(app.world().get::<bevy::light::NotShadowCaster>(shape_e).is_some());

    // ModelHasBlock now returns 1 for exact match on JG_CameraOverlayRef_PreIS.
    assert_eq!(
        num(
            &mut app,
            "ref rCam\nlet rCam := EditorIDToFormID \"JG_CameraOverlayRef_PreIS\"\nSetFunctionValue (ModelHasBlock rCam \"OverlayShape\")"
        ),
        1.0
    );

    // Apply transform + material alpha + NiAlphaProperty (blend=4845) + NiStencilProperty (item 1 drawMode=3 DRAW_BOTH) in one frame.
    eval(
        &mut app,
        "ref rCam\n\
         let rCam := EditorIDToFormID \"JG_CameraOverlayRef_PreIS\"\n\
         rCam.SetNifTrans \"OverlayNode\" 0 7.5 0 2\n\
         rCam.SetMatrPropVal \"OverlayShape\" 7 0.35\n\
         rCam.SetAlphaPropertyValue \"OverlayShape\" 0 4845\n\
         rCam.SetStencilPropertyValue \"OverlayShape\" 1 3",
    );
    app.update();

    // Transform updated on OverlayNode entity.
    let node_tf = app
        .world_mut()
        .query::<(&Name, &Transform)>()
        .iter(app.world())
        .find(|(n, _)| n.as_str() == "OverlayNode")
        .map(|(_, t)| *t)
        .expect("OverlayNode entity");
    assert_eq!(node_tf.translation, Vec3::new(0.0, 7.5, 0.0));

    // Shape received ONE private NifMaterial copy; shared original asset stays untouched.
    // Flags 4845 (0x12ED, blend+test) with the initial threshold 128 (>= BLEND_TEST_CUT) maps to Mask(128.5/255).
    let new_mat = app.world().get::<MeshMaterial3d<NifMaterial>>(shape_e).unwrap().0.clone();
    assert_ne!(new_mat, orig_mat, "script edits must clone-on-write a private material");
    {
        let materials = app.world().resource::<Assets<NifMaterial>>();
        let edited = materials.get(&new_mat).unwrap();
        let shared = materials.get(&orig_mat).unwrap();
        assert!((edited.base.base_color.alpha() - 0.35).abs() < 1e-5);
        assert_eq!(edited.base.alpha_mode, AlphaMode::Mask(128.5 / 255.0));
        assert!(edited.base.double_sided);
        assert_eq!(edited.base.cull_mode, None);
        assert_eq!(shared.base.base_color.alpha(), 1.0);
        assert_eq!(shared.base.alpha_mode, AlphaMode::Opaque);
        assert!(!shared.base.double_sided);
    }

    // Lowering NiAlphaProperty threshold (item 1) to 0 switches the same private copy to AlphaMode::Blend.
    eval(
        &mut app,
        "ref rCam\n\
         let rCam := EditorIDToFormID \"JG_CameraOverlayRef_PreIS\"\n\
         rCam.SetAlphaPropertyValue \"OverlayShape\" 1 0",
    );
    app.update();
    assert_eq!(
        app.world().get::<MeshMaterial3d<NifMaterial>>(shape_e).unwrap().0,
        new_mat,
        "subsequent alpha edit must reuse the existing private material copy"
    );
    assert_eq!(
        app.world()
            .resource::<Assets<NifMaterial>>()
            .get(&new_mat)
            .unwrap()
            .base
            .alpha_mode,
        AlphaMode::Blend
    );

    // Script getters read back the updated values after the engine publishes the next tree.
    assert_eq!(
        num(
            &mut app,
            "ref rCam\nlet rCam := EditorIDToFormID \"JG_CameraOverlayRef_PreIS\"\nSetFunctionValue (rCam.GetAlphaPropertyValue \"OverlayShape\" 0)"
        ),
        4845.0
    );
    assert_eq!(
        num(
            &mut app,
            "ref rCam\nlet rCam := EditorIDToFormID \"JG_CameraOverlayRef_PreIS\"\nSetFunctionValue (rCam.GetStencilPropertyValue \"OverlayShape\" 1)"
        ),
        3.0
    );

    // Removing the attachment (mode 2) despawns its entities and clears ModelHasBlock.
    eval(
        &mut app,
        "ref rCam\n\
         let rCam := EditorIDToFormID \"JG_CameraOverlayRef_PreIS\"\n\
         AttachModel rCam 2 \"Scene Root|overlay\\test.nif\"",
    );
    app.update();
    assert!(app.world().get_entity(shape_e).is_err());
    assert_eq!(
        num(
            &mut app,
            "ref rCam\nlet rCam := EditorIDToFormID \"JG_CameraOverlayRef_PreIS\"\nSetFunctionValue (ModelHasBlock rCam \"OverlayShape\")"
        ),
        0.0
    );
}

#[test]
fn pre_ui_overlay_activates_with_content_hides_on_loading_resizes_and_cleans_on_session_reset() {
    let (mut app, _files, _main_cam, win) = harness();

    // Merely watching PreUI via ModelHasBlock spawns the stage but keeps its offscreen camera inactive.
    assert_eq!(
        num(
            &mut app,
            "ref rCam\nlet rCam := EditorIDToFormID \"JG_CameraOverlayRef_PreUI\"\nSetFunctionValue (ModelHasBlock rCam \"OverlayShape\")"
        ),
        0.0
    );
    app.update();
    app.update();
    let (cam_active, z_index, ui_vis) = {
        let cam_active = app
            .world_mut()
            .query::<(&Camera, &CameraOverlayPreUiCamera)>()
            .single(app.world())
            .unwrap()
            .0
            .is_active;
        let (z, v, _) = app
            .world_mut()
            .query::<(&GlobalZIndex, &Visibility, &CameraOverlayPreUiNode)>()
            .single(app.world())
            .unwrap();
        (cam_active, z.0, *v)
    };
    assert!(!cam_active, "empty PreUI camera must stay inactive");
    assert_eq!(z_index, PRE_UI_Z_INDEX);
    assert_eq!(ui_vis, Visibility::Hidden);

    // Attaching a model activates the PreUI camera and shows the composite UI node.
    eval(
        &mut app,
        "ref rCam\n\
         let rCam := EditorIDToFormID \"JG_CameraOverlayRef_PreUI\"\n\
         AttachModel rCam 3 \"Scene Root|overlay\\test.nif\"",
    );
    app.update();
    app.update();
    let (cam_active, proj_fov, ui_vis, shape_layer) = {
        let (cam_active, fov) = {
            let (cam, proj, _) = app
                .world_mut()
                .query::<(&Camera, &Projection, &CameraOverlayPreUiCamera)>()
                .single(app.world())
                .unwrap();
            let fov = match proj {
                Projection::Perspective(p) => p.fov,
                _ => panic!("expected Perspective projection"),
            };
            (cam.is_active, fov)
        };
        let ui_vis = *app
            .world_mut()
            .query::<(&Visibility, &CameraOverlayPreUiNode)>()
            .single(app.world())
            .unwrap()
            .0;
        let layer = app
            .world_mut()
            .query::<(&Name, &RenderLayers)>()
            .iter(app.world())
            .find(|(n, _)| n.as_str() == "OverlayShape")
            .map(|(_, l)| l.clone())
            .expect("PreUI OverlayShape");
        (cam_active, fov, ui_vis, layer)
    };
    assert!(cam_active, "PreUI camera must activate once an overlay model is attached");
    assert!((proj_fov - 75.0f32.to_radians()).abs() < 1e-4);
    assert_eq!(ui_vis, Visibility::Inherited);
    assert_eq!(shape_layer, RenderLayers::layer(OVERLAY_PRE_UI_LAYER));

    // Also attach PreIS to verify loading-screen suppression hides BOTH stages.
    eval(
        &mut app,
        "ref rCam\n\
         let rCam := EditorIDToFormID \"JG_CameraOverlayRef_PreIS\"\n\
         AttachModel rCam 3 \"Scene Root|overlay\\test.nif\"",
    );
    app.update();
    app.world_mut().resource_mut::<TestLoading>().0 = true;
    app.update();
    for (root, vis) in app
        .world_mut()
        .query::<(&CameraOverlayRoot, &Visibility)>()
        .iter(app.world())
    {
        assert_eq!(*vis, Visibility::Hidden, "{:?} must hide during loading screen", root.stage);
    }
    let pre_ui_active_loading = app
        .world_mut()
        .query::<(&Camera, &CameraOverlayPreUiCamera)>()
        .single(app.world())
        .unwrap()
        .0
        .is_active;
    assert!(!pre_ui_active_loading, "PreUI camera must deactivate during loading screen");

    // Ending loading screen restores both stages.
    app.world_mut().resource_mut::<TestLoading>().0 = false;
    app.update();
    for (root, vis) in app
        .world_mut()
        .query::<(&CameraOverlayRoot, &Visibility)>()
        .iter(app.world())
    {
        assert_eq!(*vis, Visibility::Inherited, "{:?} must restore after loading screen", root.stage);
    }

    // Window resize replaces the target Image in-place without despawning attached shapes.
    let img_before = app
        .world_mut()
        .query::<(&ImageNode, &CameraOverlayPreUiNode)>()
        .single(app.world())
        .unwrap()
        .0
        .image
        .id();
    app.world_mut()
        .get_mut::<Window>(win)
        .unwrap()
        .resolution
        .set_physical_resolution(1920, 1080);
    app.update();
    let img_after = app
        .world_mut()
        .query::<(&ImageNode, &CameraOverlayPreUiNode)>()
        .single(app.world())
        .unwrap()
        .0
        .image
        .id();
    assert_ne!(img_before, img_after, "resize must allocate a new PreUI render target");
    assert_eq!(
        app.world().resource::<Assets<Image>>().len(),
        1,
        "old PreUI render target must be freed on resize"
    );
    assert_eq!(
        num(
            &mut app,
            "ref rCam\nlet rCam := EditorIDToFormID \"JG_CameraOverlayRef_PreUI\"\nSetFunctionValue (ModelHasBlock rCam \"OverlayShape\")"
        ),
        1.0,
        "attached overlay model must survive window resize"
    );

    // Replacing Game (save load / new session) cleanly tears down all overlay entities.
    let src: &'static Source = Box::leak(Box::new(source()));
    let fresh = Game::new(
        &src.data,
        fnv_sim::World::new(&src.forms, &src.refs, Default::default()),
        GameConfig {
            strict: true,
            ..default()
        },
    );
    app.insert_resource(TestGame(fresh));
    app.update();
    assert_eq!(
        app.world_mut()
            .query::<&CameraOverlayRoot>()
            .iter(app.world())
            .count(),
        0
    );
    assert_eq!(
        app.world_mut()
            .query::<&CameraOverlayPreUiCamera>()
            .iter(app.world())
            .count(),
        0
    );
    assert_eq!(
        app.world_mut()
            .query::<&CameraOverlayPreUiNode>()
            .iter(app.world())
            .count(),
        0
    );
}

#[test]
fn attached_overlay_particle_consumer_reads_mutates_and_isolates_from_placed_systems() {
    let (mut app, _files, _main_cam, _win) = harness();

    // Spawn a placed host reference and an overlay attachment with matching emitter names.
    let placed_root = app
        .world_mut()
        .spawn((
            Name::new("PlacedRoot"),
            RefVisual(PLACED_REF.0),
            Transform::IDENTITY,
            Visibility::Inherited,
        ))
        .id();
    let _placed_node = app
        .world_mut()
        .spawn((
            Name::new("OverlayEmitter"),
            Transform::IDENTITY,
            Visibility::Inherited,
            ChildOf(placed_root),
        ))
        .id();

    // Watch PreIS so camera_overlay spawns its root + Scene Root.
    eval(
        &mut app,
        "ref rCam\n\
         let rCam := EditorIDToFormID \"JG_CameraOverlayRef_PreIS\"\n\
         let rCam := ModelHasBlock rCam \"Scene Root\"",
    );
    app.update();
    let scene_root = app
        .world_mut()
        .query_filtered::<Entity, With<CameraOverlaySceneRoot>>()
        .single(app.world())
        .unwrap();

    // Spawn an attached particle emitter node under the overlay's Scene Root.
    let emitter_owner = app
        .world_mut()
        .spawn((
            Name::new("OverlayEmitter"),
            Transform::IDENTITY,
            Visibility::Inherited,
            ChildOf(scene_root),
        ))
        .id();
    app.world_mut().entity_mut(emitter_owner).insert(AttachedNode {
        source: nvf_assets::model::ModelObject {
            block: 1,
            name: Some("OverlayEmitter".into()),
            parent: None,
            local: Transform::IDENTITY,
            flags: 0,
            material: None,
            alpha: None,
            stencil: None,
            shape: None,
            rig_node: None,
        },
        material: None,
        emitter_owner: Some(emitter_owner),
    });

    // Seed two distinct live ParticleSystemDefs sharing the same name "OverlayEmitter":
    // one owned by the attached overlay entity, one owned by the placed reference.
    let make_def = |rate: f32, life: f32| ParticleSystemDef {
        name: "OverlayEmitter".into(),
        max: 32,
        material: Handle::default(),
        look: nvf_assets::material_anim::Look::stored([0.0; 3], 1.0, 1.0),
        emitter: Emitter {
            frame: bevy::math::Affine3A::IDENTITY,
            size: Vec3::ONE,
            round: false,
            speed: 10.0,
            speed_variation: 0.0,
            declination: 0.0,
            declination_variation: 0.0,
            planar_angle: 0.0,
            planar_angle_variation: 0.0,
            color: [1.0; 4],
            radius: 1.0,
            radius_variation: 0.0,
            life,
            life_variation: 0.0,
            mesh: None,
            keyed: Vec::new(),
            object: Some("OverlayEmitter".into()),
        },
        rate,
        rate_keys: None,
        active: vec![(0.0, true)],
        period: 1.0,
        loops: true,
        sequences: Vec::new(),
        max_emitters: 0,
        grow_time: 0.0,
        fade_time: 0.0,
        base_scale: 1.0,
        system_scale: 1.0,
        color: None,
        color_keys: None,
        rotation: None,
        gravity: Vec::new(),
        drag: Vec::new(),
        bombs: Vec::new(),
        colliders: Vec::new(),
        strip: None,
        subtextures: Vec::new(),
        mover: None,
        alpha_keys: None,
        soft: 0.0,
    };
    let mesh_a = app.world_mut().resource_mut::<Assets<Mesh>>().add(Mesh::new(
        bevy::mesh::PrimitiveTopology::TriangleList,
        bevy::asset::RenderAssetUsages::default(),
    ));
    let mesh_b = app.world_mut().resource_mut::<Assets<Mesh>>().add(Mesh::new(
        bevy::mesh::PrimitiveTopology::TriangleList,
        bevy::asset::RenderAssetUsages::default(),
    ));
    let dummy_a = app.world_mut().spawn_empty().id();
    let dummy_b = app.world_mut().spawn_empty().id();
    {
        let mut particles = app.world_mut().resource_mut::<crate::particles::Particles>();
        crate::particles::seed_test_system(
            &mut particles,
            emitter_owner,
            CAMERA_OVERLAY_PRE_IS.0,
            true,
            "meshes/overlay/particles.nif",
            make_def(42.0, 2.5),
            mesh_a,
            dummy_a,
        );
        crate::particles::seed_test_system(
            &mut particles,
            placed_root,
            PLACED_REF.0,
            false,
            "meshes/placed/particles.nif",
            make_def(99.0, 7.0),
            mesh_b,
            dummy_b,
        );
    }

    // Publish both trees to the script mirror.
    app.update();
    assert_eq!(
        num(
            &mut app,
            "ref rCam\nlet rCam := EditorIDToFormID \"JG_CameraOverlayRef_PreIS\"\nSetFunctionValue (rCam.GetParticleEmitterSpawnRate \"OverlayEmitter\")"
        ),
        42.0
    );
    assert_eq!(
        num(
            &mut app,
            "ref rCam\nlet rCam := EditorIDToFormID \"JG_CameraOverlayRef_PreIS\"\nSetFunctionValue (rCam.GetNiPSysModifierValue \"OverlayEmitter\" 14)"
        ),
        2.5
    );

    // Mutate spawn rate (id 20) and life (id 14) on the camera overlay emitter.
    assert_eq!(
        num(
            &mut app,
            "ref rCam\nlet rCam := EditorIDToFormID \"JG_CameraOverlayRef_PreIS\"\nSetFunctionValue (rCam.SetParticleEmitterSpawnRate \"OverlayEmitter\" 0)"
        ),
        1.0
    );
    assert_eq!(
        num(
            &mut app,
            "ref rCam\nlet rCam := EditorIDToFormID \"JG_CameraOverlayRef_PreIS\"\nSetFunctionValue (rCam.SetNiPSysModifierValue \"OverlayEmitter\" 14 6)"
        ),
        1.0
    );
    app.update();

    // Verify ONLY the overlay's attached ParticleSystemDef changed; placed system untouched!
    let mut particles = app.world_mut().resource_mut::<crate::particles::Particles>();
    let overlay_sys = crate::particles::instance_systems(&mut particles, emitter_owner);
    assert_eq!(overlay_sys.len(), 1);
    assert_eq!(overlay_sys[0].rate, 0.0);
    assert_eq!(overlay_sys[0].emitter.life, 6.0);
    let placed_sys = crate::particles::placed_systems(&mut particles, PLACED_REF.0);
    assert_eq!(placed_sys.len(), 1);
    assert_eq!(placed_sys[0].rate, 99.0);
    assert_eq!(placed_sys[0].emitter.life, 7.0);
}

#[test]
fn stencil_draw_modes_match_at_spawn_and_after_set_stencil_property_value() {
    use bevy::render::render_resource::Face;

    let (mut app, _files, _main_cam, _win) = harness();

    let expected = [
        (0u16, (false, Some(Face::Back))),
        (1u16, (false, Some(Face::Back))),
        (2u16, (false, Some(Face::Front))),
        (3u16, (true, None)),
    ];

    for (mode, want) in expected {
        let flags = (mode << 10) | 0x0015;
        let path = format!("overlay\\stencil_{mode}.nif");
        eval(
            &mut app,
            &format!(
                "ref rCam\n\
                 let rCam := EditorIDToFormID \"JG_CameraOverlayRef_PreIS\"\n\
                 AttachModel rCam 3 \"Scene Root|{path}\""
            ),
        );
        app.update();
        app.update();

        let (shape_e, spawn_mat) = app
            .world_mut()
            .query::<(Entity, &Name, &MeshMaterial3d<NifMaterial>)>()
            .iter(app.world())
            .find(|(_, name, _)| name.as_str() == "OverlayShape")
            .map(|(e, _, m)| (e, m.0.clone()))
            .expect("OverlayShape spawned");

        let at_spawn = {
            let m = app
                .world()
                .resource::<Assets<NifMaterial>>()
                .get(&spawn_mat)
                .expect("spawned material");
            (m.base.double_sided, m.base.cull_mode)
        };
        assert_eq!(
            at_spawn, want,
            "spawn-time NiStencilProperty draw mode {mode} (flags {flags:#x}) must match unified mapping"
        );

        // Setting the exact same draw mode (item 1) through SetStencilPropertyValue must yield the exact
        // same material double_sided / cull_mode as at spawn.
        eval(
            &mut app,
            &format!(
                "ref rCam\n\
                 let rCam := EditorIDToFormID \"JG_CameraOverlayRef_PreIS\"\n\
                 rCam.SetStencilPropertyValue \"OverlayShape\" 1 {mode}"
            ),
        );
        app.update();

        let after_handle = app
            .world()
            .get::<MeshMaterial3d<NifMaterial>>(shape_e)
            .expect("shape material after SetStencilPropertyValue")
            .0
            .clone();
        let after_set = {
            let m = app
                .world()
                .resource::<Assets<NifMaterial>>()
                .get(&after_handle)
                .expect("updated material");
            (m.base.double_sided, m.base.cull_mode)
        };
        assert_eq!(
            after_set, at_spawn,
            "SetStencilPropertyValue with identical flags ({flags:#x}, draw mode {mode}) must preserve spawn material culling"
        );
        assert_eq!(
            after_set,
            nvf_assets::nif_spawn::stencil_cull(i64::from(flags)),
            "SetStencilPropertyValue must match nif_spawn::stencil_cull for draw mode {mode}"
        );

        eval(
            &mut app,
            &format!(
                "ref rCam\n\
                 let rCam := EditorIDToFormID \"JG_CameraOverlayRef_PreIS\"\n\
                 AttachModel rCam 2 \"Scene Root|{path}\""
            ),
        );
        app.update();
    }
}
