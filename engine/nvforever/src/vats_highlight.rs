//! The classic VATS style's selected body part lit on the target (docs/design/
//! vats-classic.md §12).
//!
//! While targets are chosen, the part the next attack would be queued on is covered on
//! the target's own body in the interface's colour with level lines through it and a hard
//! line round it [observed: the owner's screenshots of the original's VATS, 2026-11-10: a
//! Jackal Gang Member's and a raider's head, flat colour, fine lines, a crisp outline].
//!
//! It is a piece of the HUD, not of the picture: the picture's depth of field, bloom and
//! smoothing do not touch it [design: the fork owner 2026-11-10, after three tries drawn
//! in the world, each of which came out soft: "move all the VATS/Dead Eye HUD elements to
//! the same level as rest of the hud elements so they are not effected by effects like
//! bloom/depth of field"].
//!
//! - **The mask**: a camera of its own at the player's camera ([`MASK_LAYER`], the same
//!   place and field of view) draws into a picture the size of the window: every drawn
//!   shape of the target in black, so the target hides its own parts as it does in the
//!   world (a hand before a head), and over them the selected part in white. Nothing else
//!   of the world is in it: the part shows through whatever stands between it and the
//!   player.
//! - **How much of a vertex is the part's**: a body part is a chain of the skeleton's
//!   bones (`crate::limbs::LimbRig::chains`); a bone in no chain goes with the nearest
//!   chain above it (a hand with its arm, the fingers with the hand). A skinned vertex is
//!   the part's by the share of its weights on the part's bones, and the part ends where
//!   that share is a half; a shape hung from one bone (a hat, a horn) is the part's whole
//!   when that bone is. The bones are the ones the hit volumes use, so what is lit is what
//!   a shot at the part can hit [design: the original's rule is not known]. A target
//!   without parts is lit whole.
//! - **Hair, beards, lashes**: a shape drawn through its texture's alpha is in the mask
//!   through that texture, so hair is lit as hair and not as the cards it is drawn on.
//! - **On the screen**: one node over the whole view, under the target screen's boxes,
//!   drawn from the mask by `vats_mask.wesl`: the fill with its lines where the mask is
//!   white, the outline where it is not but is within the line's width of where it is.
//!   The fill and the outline are the HUD's colour (`hud_rgb`), as the boxes are
//!   [design: the fork owner 2026-11-10, "make the inner colored stripes follow the main
//!   HUD color"]. The outline is the mask grown by a round brush, its edge smoothed over a
//!   pixel, so it runs on round a corner instead of ending in a point [design: the same
//!   day, "the corners/ends of the highlight stroke are very sharp, could they be more
//!   connected/less sharp"].
//! - **The weapon in its hand** (`crate::weapons::HeldWeapon`, the part numbered
//!   `hitvol::WEAPON_PART`): its shapes are in the mask as the body's are, in black, and in
//!   white when the weapon is the part selected; a shape on its back (`OnBack`, the
//!   flamer's tank) is no part of that, as it is none of the weapon's hit volume.
//! - **It pulses** as the enemy bar's ticks for the damage in sight do, with them
//!   (`nvf_play::hud::sight_alpha`, real time) [design: the fork owner 2026-11-10, "make
//!   the highlighted bodypart pulse the same way the predicted damage on healthbar does"]:
//!   the fill and the outline both.
//! - **Life**: the shapes' copies are made when the selected target or part changes and
//!   removed when the screen goes; the camera draws only meanwhile.
//!
//! Log: `vats highlight: <actor> part <n>: <t> triangles on <s> shapes`, `vats highlight:
//! off`, and at exit `vats highlight summary: <n> shown`.

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::{NoFrustumCulling, RenderLayers};
use bevy::camera::{ImageRenderTarget, RenderTarget};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::mesh::skinning::SkinnedMesh;
use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Extent3d, ShaderType, TextureDimension, TextureFormat, TextureUsages};
use bevy::shader::ShaderRef;
use bevy::ui_render::prelude::{MaterialNode, UiMaterial};
use bevy::window::PrimaryWindow;
use nvf_assets::actor::{skin_weights, LimbShape};
use nvf_assets::NifMaterial;
use nvf_play::vats_mode::VatsMode;
use nvf_play::RefVisual;

use crate::limbs::{ActorRig, LimbRig};
use crate::weapons::{HeldWeapon, OnBack};
use nvf_play::hitvol::WEAPON_PART;

/// The render layer the mask's camera sees and nothing else does.
pub const MASK_LAYER: usize = 15;
/// How much of the amber covers the part, and what is left of that on each line's faint
/// half [design: by eye against the owner's screenshots; "the inner line should have less
/// opacity to not be so bright"].
pub const STRENGTH: f32 = 0.5;
pub const SCAN_FAINT: f32 = 0.3;
/// The lines [observed: the owner's screenshots of the original's VATS, some 6 pixels
/// apart at 720 lines]: their period as a part of the view's height, and the least in
/// pixels.
pub const SCAN_PERIOD: f32 = 6.0 / 720.0;
pub const SCAN_LEAST: f32 = 4.0;
/// The outline's width as a part of the view's height, and the least in pixels [design:
/// "a hard outline around the whole bodypart"; some 3 pixels at 720 lines on the
/// screenshots].
pub const OUTLINE_SHARE: f32 = 0.004;
pub const OUTLINE_LEAST: f32 = 2.0;
/// A point of the mask is the part's above this (the part's edge is where a vertex's share
/// is a half; a hair's strand counts from a quarter of its texture's alpha).
pub const MASK_ON: f32 = 0.25;
/// A shape drawn through its texture's alpha is in the mask where that alpha is above
/// this, when its own material names no cut [design].
const SOFT_CUT: f32 = 0.12;
/// The light the part's white is drawn with in the mask ([`coats`]).
const MASK_LIGHT: f32 = 1.0e6;
/// The node's stacking level: under the target screen's boxes
/// (`nvf_play::tiles::vats_menu::VATS_Z`), over the marks.
const MASK_Z: i32 = nvf_play::tiles::vats_menu::VATS_Z - 1;

const SHADER: &str = "embedded://nvforever/vats_mask.wesl";

/// The numbers of `vats_mask.wesl`.
#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
pub struct MaskUniform {
    /// The fill's colour and how much of it covers the part.
    pub fill: Vec4,
    /// The outline's colour.
    pub line: Vec4,
    /// A pixel of the mask as a part of it (x, y), the outline's width and the lines'
    /// period, both in pixels.
    pub shape: Vec4,
    /// What is left of the fill on a line's faint half, above what the mask is the
    /// part's, and how much of the fill and the outline is drawn now (the pulse).
    pub more: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct PartMaskMaterial {
    #[uniform(0)]
    pub u: MaskUniform,
    #[texture(1)]
    #[sampler(2)]
    pub mask: Handle<Image>,
}

impl UiMaterial for PartMaskMaterial {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
}

/// Marks a shape's copy in the mask.
#[derive(Component)]
struct PartGlow;

/// The mask's camera, its picture and the node drawn from it.
struct View {
    camera: Entity,
    node: Entity,
    image: Handle<Image>,
    material: Handle<PartMaskMaterial>,
    size: (u32, u32),
}

#[derive(Resource, Default)]
struct Glow {
    /// The target and part lit now.
    on: Option<(u32, Option<u8>)>,
    /// The black and the white a plain shape is drawn in.
    plain: Option<[Handle<StandardMaterial>; 2]>,
    /// The same for a shape drawn through its texture, by the texture and its cut.
    cut: std::collections::HashMap<(AssetId<Image>, u8), [Handle<StandardMaterial>; 2]>,
    overlays: Vec<Entity>,
    shown: u32,
    view: Option<View>,
}

impl Glow {
    /// The black and the white for a shape that stands in the mask as `cover`; `None` for
    /// one that does not stand in it.
    fn coats_for(&mut self, materials: &mut Assets<StandardMaterial>, cover: Cover) -> Option<[Handle<StandardMaterial>; 2]> {
        match cover {
            Cover::None => None,
            Cover::Plain => Some(self.plain.get_or_insert_with(|| coats(materials, None)).clone()),
            Cover::Through(tex, cut) => Some(self.cut.entry((tex.id(), (cut * 255.0).round() as u8)).or_insert_with(|| coats(materials, Some((tex, cut)))).clone()),
        }
    }
}

pub struct VatsHighlightPlugin;

impl Plugin for VatsHighlightPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "vats_mask.wesl");
        app.add_plugins(bevy::ui_render::UiMaterialPlugin::<PartMaskMaterial>::default())
            .init_resource::<Glow>()
            .add_systems(Update, (glow, view).chain().after(nvf_play::vats_mode::VatsUpdate))
            .add_systems(Last, report);
    }
}

/// The part each skeleton node goes with: its chain's, else that of the nearest node above
/// it that is in a chain; `None` for a node above every chain.
pub fn node_parts(parents: &[Option<usize>], chains: &[(u8, Vec<usize>)]) -> Vec<Option<u8>> {
    let mut own: Vec<Option<u8>> = vec![None; parents.len()];
    for (part, nodes) in chains {
        for &n in nodes {
            if let Some(slot) = own.get_mut(n) {
                *slot = Some(*part);
            }
        }
    }
    (0..parents.len())
        .map(|i| {
            let mut at = Some(i);
            let mut steps = 0;
            while let Some(n) = at {
                if let Some(p) = own.get(n).copied().flatten() {
                    return Some(p);
                }
                at = parents.get(n).copied().flatten();
                steps += 1;
                if steps > 256 {
                    break;
                }
            }
            None
        })
        .collect()
}

/// How much of a vertex is the part's: the share of its weights on the part's joints
/// (`lit[j]` for joint j).
pub fn vertex_share(joints: &[u16; 4], weights: &[f32; 4], lit: &[bool]) -> f32 {
    let (mut on, mut all) = (0.0, 0.0);
    for k in 0..4 {
        let w = weights[k].max(0.0);
        all += w;
        if lit.get(joints[k] as usize).copied().unwrap_or(false) {
            on += w;
        }
    }
    if all <= 0.0 {
        return 0.0;
    }
    (on / all).clamp(0.0, 1.0)
}

/// The triangles of `indices` (three each) with any corner that has some of the part.
pub fn touched_triangles(indices: &[u32], share: &[f32]) -> Vec<u32> {
    indices.as_chunks::<3>().0.iter().filter(|t| t.iter().any(|&v| share.get(v as usize).copied().unwrap_or(0.0) > 0.02)).flatten().copied().collect()
}

fn mesh_indices(m: &Mesh) -> Vec<u32> {
    match m.indices() {
        Some(Indices::U32(i)) => i.clone(),
        Some(Indices::U16(i)) => i.iter().map(|&x| u32::from(x)).collect(),
        None => Vec::new(),
    }
}

/// How a shape stands in the mask, from its own material.
#[derive(Debug, PartialEq)]
enum Cover {
    /// Whole.
    Plain,
    /// Through its texture's alpha, above the cut: a cut-out or blended shape (hair, a
    /// beard, lashes), so it hides and is lit as what is seen of it.
    Through(Handle<Image>, f32),
    /// Not at all: a refracting shape (heat haze) shows no surface.
    None,
}

fn cover_of(m: &NifMaterial) -> Cover {
    if m.extension.refract {
        return Cover::None;
    }
    let cut = match m.base.alpha_mode {
        AlphaMode::Opaque if m.extension.alpha_cut <= 0.0 => return Cover::Plain,
        AlphaMode::Opaque => m.extension.alpha_cut,
        AlphaMode::Mask(c) => c,
        _ => SOFT_CUT,
    };
    match &m.base.base_color_texture {
        Some(t) => Cover::Through(t.clone(), cut.clamp(0.02, 0.98)),
        None => Cover::Plain,
    }
}

/// The black a shape hides with and the white the part is drawn in, for a shape drawn
/// whole or through `texture`. The white is blended, after the black and at its depth, so
/// the part always lies over its own shape's black; its alpha carries the vertices' share.
fn coats(materials: &mut Assets<StandardMaterial>, texture: Option<(Handle<Image>, f32)>) -> [Handle<StandardMaterial>; 2] {
    let (image, mode) = match texture {
        Some((t, cut)) => (Some(t), AlphaMode::Mask(cut)),
        None => (None, AlphaMode::Opaque),
    };
    let black = StandardMaterial { base_color: Color::BLACK, base_color_texture: image.clone(), unlit: true, alpha_mode: mode, ..default() };
    // The white is the material's own light, not its colour: a colour is multiplied by
    // the texture's (dark hair came out dark in the mask and was not lit), a light is
    // not, and the texture still gives the alpha. Far more than the camera's exposure
    // takes off, so it is white in the picture.
    let white = StandardMaterial { base_color: Color::BLACK, base_color_texture: image, emissive: LinearRgba::rgb(MASK_LIGHT, MASK_LIGHT, MASK_LIGHT), alpha_mode: AlphaMode::Blend, ..default() };
    [materials.add(black), materials.add(white)]
}

/// Put the selected target's shapes into the mask (the module's notes) when the target or
/// the part changes.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn glow(
    mut commands: Commands,
    vats: Option<Res<VatsMode>>,
    mut st: ResMut<Glow>,
    rigs: Query<(&ActorRig, Option<&LimbRig>)>,
    shapes: Query<(Entity, &RefVisual, &LimbShape, Option<&SkinnedMesh>, Option<&MeshMaterial3d<NifMaterial>>, Option<&Mesh3d>)>,
    held: Query<(Entity, &HeldWeapon, Option<&MeshMaterial3d<NifMaterial>>, &Mesh3d, Has<OnBack>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    own_materials: Res<Assets<NifMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let want = vats.as_deref().and_then(|v| v.highlighted());
    if want == st.on {
        return;
    }
    for e in st.overlays.drain(..) {
        commands.entity(e).try_despawn();
    }
    let was = st.on.is_some();
    st.on = want;
    let Some((actor, part)) = want else {
        if was {
            info!("vats highlight: off");
        }
        return;
    };
    let Some((rig, limb)) = rigs.iter().find(|(r, _)| r.reference == actor) else { return };
    // The part each joint name goes with.
    let chains: Vec<(u8, Vec<usize>)> =
        limb.map(|l| l.chains.iter().map(|(p, _, nodes, _)| (*p, nodes.iter().filter_map(|e| rig.nodes.iter().position(|n| n == e)).collect())).collect()).unwrap_or_default();
    let own = node_parts(&rig.skel.parents, &chains);
    let part_of = |bone: &str| -> Option<u8> {
        let n = rig.skel.by_name.get(bone).copied().or_else(|| rig.skel.names.iter().position(|x| x.as_deref().is_some_and(|x| x.eq_ignore_ascii_case(bone))))?;
        own.get(n).copied().flatten()
    };
    let layer = RenderLayers::layer(MASK_LAYER);
    let (mut triangles, mut covered) = (0, 0);
    for (e, r, shape, skin, own_material, drawn) in &shapes {
        if r.0 != actor || shape.cap {
            continue;
        }
        let cover = own_material.and_then(|m| own_materials.get(&m.0)).map_or(Cover::Plain, cover_of);
        debug!("vats highlight: shape {e} on {:?}: {cover:?}", shape.bones.first());
        let Some([black, white]) = st.coats_for(&mut materials, cover) else { continue };
        let put = |commands: &mut Commands, mesh: Handle<Mesh>, material: Handle<StandardMaterial>| {
            let mut c = commands.spawn((Mesh3d(mesh), MeshMaterial3d(material), Transform::IDENTITY, PartGlow, layer.clone(), NoFrustumCulling, bevy::light::NotShadowCaster));
            if let Some(skin) = skin {
                c.insert(skin.clone());
            }
            let x = c.id();
            commands.entity(e).add_child(x);
            x
        };
        // The shape as it is drawn, in black: the target hides its own parts.
        if let Some(d) = drawn {
            let x = put(&mut commands, d.0.clone(), black);
            st.overlays.push(x);
        }
        // A shape all of whose bones are the part's (a head, its hair, eyes and hat; any
        // shape of a target without parts) is the part's whole: the mesh it is drawn with
        // is drawn once more in white. That mesh is the one on the screen; the shape's
        // first mesh is not always (a face and its hair are shaped by the face's morphs,
        // and a part made from the first mesh lay inside them: a head lit only round the
        // mouth, hair left dark) [observed: the owner's screenshots and the engine's].
        let lit: Vec<bool> = shape.bones.iter().map(|b| part.is_none_or(|p| part_of(b) == Some(p))).collect();
        if let (Some(d), true) = (drawn, !lit.is_empty() && lit.iter().all(|l| *l)) {
            let x = put(&mut commands, d.0.clone(), white);
            st.overlays.push(x);
            triangles += mesh_indices(&shape.cpu).len() / 3;
            covered += 1;
            continue;
        }
        if !lit.iter().any(|l| *l) {
            continue;
        }
        // A shape only part of which is the part's (a body under several parts): its
        // share, in white, from the mesh as it is drawn now where the main world still
        // holds it, else from its first one.
        let base: Mesh = drawn.and_then(|d| meshes.get(&d.0)).filter(|m| m.try_indices().is_ok()).cloned().unwrap_or_else(|| (*shape.cpu).clone());
        let all = mesh_indices(&base);
        let vertices = base.count_vertices();
        let (kept, share) = match (part, skin_weights(&base)) {
            // A target without parts is lit whole.
            (None, _) => (all, vec![1.0; vertices]),
            (Some(_), Some((joints, weights))) => {
                let share: Vec<f32> = joints.iter().zip(weights).map(|(j, w)| vertex_share(j, w, &lit)).collect();
                (touched_triangles(&all, &share), share)
            }
            // (Hung from one bone, it was the part's whole or none of it above.)
            (Some(_), None) => (Vec::new(), Vec::new()),
        };
        if kept.is_empty() || share.len() != vertices {
            continue;
        }
        triangles += kept.len() / 3;
        covered += 1;
        let mut mesh = base;
        mesh.insert_indices(Indices::U32(kept));
        // The share as the vertex colour's alpha: the white is blended by it.
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, share.iter().map(|&s| [1.0, 1.0, 1.0, s]).collect::<Vec<[f32; 4]>>());
        let x = put(&mut commands, meshes.add(mesh), white);
        st.overlays.push(x);
    }
    // The weapon in its hand (the module's notes): rigid shapes under the skeleton's
    // weapon node, drawn once more where they are.
    let weapon_lit = part.is_none_or(|p| p == WEAPON_PART);
    for (e, h, own_material, drawn, on_back) in &held {
        if h.reference != actor {
            continue;
        }
        let cover = own_material.and_then(|m| own_materials.get(&m.0)).map_or(Cover::Plain, cover_of);
        let Some([black, white]) = st.coats_for(&mut materials, cover) else { continue };
        let layers = if weapon_lit && !on_back { vec![black, white] } else { vec![black] };
        covered += usize::from(layers.len() == 2);
        for material in layers {
            let x = commands.spawn((Mesh3d(drawn.0.clone()), MeshMaterial3d(material), Transform::IDENTITY, PartGlow, layer.clone(), NoFrustumCulling, bevy::light::NotShadowCaster)).id();
            commands.entity(e).add_child(x);
            st.overlays.push(x);
        }
    }
    st.shown += 1;
    info!("vats highlight: {actor:08X} part {}: {triangles} triangles on {covered} shapes", part.map_or_else(|| "whole".to_string(), |p| p.to_string()));
}

/// The numbers the node is drawn with for a mask `size` pixels, the HUD's colour and how
/// much of the part is drawn now (`pulse`, 0 to 1).
pub fn mask_uniform(size: (u32, u32), hud: [u8; 3], pulse: f32) -> MaskUniform {
    let (w, h) = (size.0.max(1) as f32, size.1.max(1) as f32);
    let [r, g, b] = hud.map(|v| f32::from(v) / 255.0);
    let line = Color::srgb(r, g, b).to_linear();
    let fill = line;
    MaskUniform {
        fill: Vec4::new(fill.red, fill.green, fill.blue, STRENGTH),
        line: Vec4::new(line.red, line.green, line.blue, 1.0),
        shape: Vec4::new(1.0 / w, 1.0 / h, (h * OUTLINE_SHARE).round().max(OUTLINE_LEAST), (h * SCAN_PERIOD).round().max(SCAN_LEAST)),
        more: Vec4::new(SCAN_FAINT, MASK_ON, pulse.clamp(0.0, 1.0), 0.0),
    }
}

/// Keep the mask's camera, its picture and the node drawn from it (the module's notes):
/// made the first time a part is lit, the camera at the player's with its field of view,
/// the picture the window's size, both at work only while a part is lit.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn view(
    mut commands: Commands,
    mut st: ResMut<Glow>,
    windows: Query<&Window, With<PrimaryWindow>>,
    player: Query<(Entity, &Projection), With<nvf_play::PlayerCamera>>,
    mut cameras: Query<(&mut Camera, &mut Projection), Without<nvf_play::PlayerCamera>>,
    mut nodes: Query<&mut Visibility, With<MaterialNode<PartMaskMaterial>>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<PartMaskMaterial>>,
    real: Res<Time<Real>>,
) {
    let on = st.on.is_some();
    let pulse = nvf_play::hud::sight_alpha(real.elapsed_secs());
    let (Ok(window), Ok((eye, projection))) = (windows.single(), player.single()) else { return };
    let size = (window.physical_width().max(1), window.physical_height().max(1));
    if st.view.is_none() {
        if !on {
            return;
        }
        let extent = Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 };
        let mut image = Image::new_fill(extent, TextureDimension::D2, &[0, 0, 0, 0], TextureFormat::Bgra8UnormSrgb, RenderAssetUsages::default());
        image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
        let image = images.add(image);
        let camera = commands
            .spawn((
                Name::new("VATS part mask camera"),
                Camera3d::default(),
                Camera { order: -6, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
                RenderTarget::Image(ImageRenderTarget::from(image.clone())),
                projection.clone(),
                Tonemapping::None,
                Msaa::Off,
                Transform::IDENTITY,
                RenderLayers::layer(MASK_LAYER),
                ChildOf(eye),
            ))
            .id();
        let material = materials.add(PartMaskMaterial { u: mask_uniform(size, nvf_play::interface::hud_rgb(), pulse), mask: image.clone() });
        let whole = Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() };
        let node = commands.spawn((Name::new("VATS lit part"), whole, MaterialNode(material.clone()), GlobalZIndex(MASK_Z), bevy::picking::Pickable::IGNORE)).id();
        st.view = Some(View { camera, node, image, material, size });
        info!("vats highlight: the mask's camera and its picture ({} x {})", size.0, size.1);
        return;
    }
    let Some(v) = st.view.as_mut() else { return };
    // The window's size and the player's field of view, as they are now.
    if v.size != size {
        if let Some(mut image) = images.get_mut(&v.image) {
            image.resize(Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 });
        }
        v.size = size;
    }
    if let Ok((mut camera, mut own)) = cameras.get_mut(v.camera) {
        if camera.is_active != on {
            camera.is_active = on;
        }
        if on {
            *own = projection.clone();
        }
    }
    if let Ok(mut shown) = nodes.get_mut(v.node) {
        shown.set_if_neq(if on { Visibility::Inherited } else { Visibility::Hidden });
    }
    let u = mask_uniform(size, nvf_play::interface::hud_rgb(), pulse);
    if on && materials.get(&v.material).is_some_and(|m| m.u != u) {
        if let Some(mut m) = materials.get_mut(&v.material) {
            m.u = u;
        }
    }
}

fn report(mut exit: MessageReader<AppExit>, st: Res<Glow>) {
    if exit.read().next().is_some() {
        info!("vats highlight summary: {} shown", st.shown);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bone_outside_every_chain_goes_with_the_chain_above_it() {
        // 0 root, 1 spine (torso), 2 arm (arm), 3 hand, 4 finger, 5 head (head), 6 tail.
        let parents = [None, Some(0), Some(1), Some(2), Some(3), Some(1), Some(0)];
        let own = node_parts(&parents, &[(0, vec![1]), (3, vec![2]), (1, vec![5])]);
        assert_eq!(own, [None, Some(0), Some(3), Some(3), Some(3), Some(1), None]);
    }

    #[test]
    fn a_vertex_is_the_parts_by_its_weights_on_the_parts_bones() {
        let lit = [true, false, true];
        assert_eq!(vertex_share(&[0, 2, 0, 0], &[0.6, 0.4, 0.0, 0.0], &lit), 1.0);
        assert_eq!(vertex_share(&[1, 1, 0, 0], &[0.6, 0.4, 0.0, 0.0], &lit), 0.0);
        assert!((vertex_share(&[0, 1, 0, 0], &[0.25, 0.75, 0.0, 0.0], &lit) - 0.25).abs() < 1e-6);
        assert_eq!(vertex_share(&[0, 1, 0, 0], &[0.0; 4], &lit), 0.0, "a vertex no joint carries");
    }

    #[test]
    fn a_triangle_is_drawn_when_any_corner_has_some_of_the_part() {
        let indices = [0, 1, 2, 2, 3, 4, 4, 5, 6];
        let share = [1.0, 1.0, 0.3, 0.0, 0.01, 0.0, 0.0];
        assert_eq!(touched_triangles(&indices, &share), [0, 1, 2, 2, 3, 4], "the third has too little");
        assert!(touched_triangles(&indices, &[0.0; 7]).is_empty());
    }

    #[test]
    fn the_nodes_numbers_follow_the_windows_size_and_the_huds_colour() {
        let u = mask_uniform((1280, 720), [255, 255, 255], 0.4);
        assert_eq!(u.more.z, 0.4, "the pulse");
        assert_eq!((u.shape.x, u.shape.y), (1.0 / 1280.0, 1.0 / 720.0));
        assert_eq!((u.shape.z, u.shape.w), (3.0, 6.0), "3 pixels of outline and lines 6 apart at 720 lines");
        assert_eq!((u.line.x, u.line.y, u.line.z), (1.0, 1.0, 1.0), "the outline in the HUD's colour");
        let blue = mask_uniform((1280, 720), [0, 0, 255], 1.0);
        assert_eq!((blue.fill.x, blue.fill.y, blue.fill.z), (blue.line.x, blue.line.y, blue.line.z), "and the fill's lines in it too");
        assert_eq!(u.fill.w, STRENGTH);
        // Twice the size: twice the pixels, the same look.
        let big = mask_uniform((2560, 1440), [255, 255, 255], 1.0);
        assert_eq!((big.shape.z, big.shape.w), (6.0, 12.0));
        // A tiny window keeps the least.
        let small = mask_uniform((320, 240), [255, 255, 255], 1.0);
        assert_eq!((small.shape.z, small.shape.w), (OUTLINE_LEAST, SCAN_LEAST));
    }
}
