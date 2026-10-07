//! NifAssetPlugin: spawn a NIF's scene graph as Bevy entities.
//!
//! Conventions [observed, docs/spec-notes/nif.md §7]: rotation matrices are stored row by
//! row and rotate column vectors; a child's world transform is parent ∘ child; triangles
//! are counter-clockwise when seen from the side their normals face; a bone's skin
//! transform is inverse(bone bind world) ∘ shape bind world, which is exactly the
//! inverse bind pose Bevy expects. Gamebryo is Z-up, Bevy Y-up: the root entity rotates
//! by −90° about X.

use crate::data::GameData;
use crate::texture::TextureCache;
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::DynamicSkinnedMeshBounds;
use bevy::ecs::system::SystemParam;
use bevy::image::CompressedImageFormatSupport;
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::math::Affine3A;
use bevy::prelude::*;
use fnv_nif::scene::{mesh_data, scene_objects, Decoded, SceneObject, Transform as NifTransform};
use fnv_nif::{NifFile, Struct, Value};
use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;

/// Marks the root of a spawned NIF.
#[derive(Component)]
pub struct NifRoot;

/// Set on objects whose NiAVObject flags have bit 0 set [hypothesis: "hidden"; editor
/// markers and collision helpers carry it].
#[derive(Component)]
pub struct NifHidden;

/// Summary of what was spawned, for the overlay and for framing the camera.
#[derive(Debug, Clone, Default)]
pub struct SpawnReport {
    pub blocks: usize,
    pub shapes: usize,
    pub skinned: usize,
    pub triangles: usize,
    pub textures_missing: Vec<String>,
    /// Bounds in Bevy world space (after the Z-up → Y-up rotation).
    pub bounds: Option<(Vec3, Vec3)>,
    /// Node name → entity, for animation targets.
    pub nodes: HashMap<String, Entity>,
    pub root: Option<Entity>,
    /// Skeleton file the skinned shapes were bound to.
    pub skeleton: Option<String>,
}

#[derive(SystemParam)]
pub struct SpawnCtx<'w, 's> {
    pub commands: Commands<'w, 's>,
    pub meshes: ResMut<'w, Assets<Mesh>>,
    pub materials: ResMut<'w, Assets<StandardMaterial>>,
    pub images: ResMut<'w, Assets<Image>>,
    pub bindposes: ResMut<'w, Assets<SkinnedMeshInverseBindposes>>,
    pub textures: ResMut<'w, TextureCache>,
    pub data: Res<'w, GameData>,
    pub formats: Option<Res<'w, CompressedImageFormatSupport>>,
}

pub fn to_bevy(t: &NifTransform) -> Transform {
    let m = Mat3::from_cols_array(&t.rotation).transpose(); // stored row-major
    Transform { translation: Vec3::from_array(t.translation), rotation: Quat::from_mat3(&m).normalize(), scale: Vec3::splat(t.scale) }
}

fn affine(t: &NifTransform) -> Affine3A {
    let m = Mat3::from_cols_array(&t.rotation).transpose();
    Affine3A::from_mat3_translation(m * t.scale, Vec3::from_array(t.translation))
}

/// Rendering-relevant properties accumulated from an object and its ancestors.
#[derive(Debug, Clone, Default)]
struct Props {
    textures: Vec<String>,
    alpha: Option<(i64, i64)>,
    material_alpha: Option<f32>,
    emissive: Option<[f32; 3]>,
    glossiness: Option<f32>,
    double_sided: bool,
    vertex_colors_tint: bool,
}

fn text_list(s: &Struct, name: &str) -> Vec<String> {
    s.list(name)
        .unwrap_or_default()
        .iter()
        .map(|v| match v {
            Value::Str { text: Some(t), .. } => t.clone(),
            _ => String::new(),
        })
        .collect()
}

fn apply_property(file: &NifFile, dec: &Decoded, r: i32, p: &mut Props) {
    let Some(s) = dec.get(r) else { return };
    let ty = file.blocks[r as usize].type_name.as_str();
    if let Some(ts) = s.block_ref("texture_set").and_then(|t| dec.get(t)) {
        p.textures = text_list(ts, "textures");
    }
    match ty {
        "BSShaderNoLightingProperty" => {
            if let Some(f) = s.text("file_name") {
                p.textures = vec![f.to_string()];
            }
        }
        "NiTexturingProperty" => {
            let src = s.strukt("base_texture").and_then(|b| b.block_ref("source")).and_then(|r| dec.get(r));
            if let Some(f) = src.and_then(|t| t.text("file_name")) {
                p.textures = vec![f.to_string()];
            }
        }
        "NiAlphaProperty" => p.alpha = Some((s.int("flags").unwrap_or(0), s.int("threshold").unwrap_or(128))),
        "NiMaterialProperty" => {
            p.material_alpha = s.float("alpha");
            p.emissive = s.floats("emissive_color").and_then(|e| e.try_into().ok());
            p.glossiness = s.float("glossiness");
        }
        "NiStencilProperty" => {
            // 20.1.0.3+: draw mode in flags bits 10-11; older files: draw_mode field.
            let mode = s.int("draw_mode").or_else(|| s.int("flags").map(|f| (f >> 10) & 3)).unwrap_or(0);
            p.double_sided = mode == 3;
        }
        "NiVertexColorProperty" => p.vertex_colors_tint = true,
        _ => {}
    }
}

fn build_mesh(data: &Struct) -> Option<(Mesh, usize)> {
    let md = mesh_data(data);
    if md.positions.is_empty() || md.triangles.is_empty() {
        return None;
    }
    let n = md.positions.len();
    let tris = md.triangles.len();
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, md.positions);
    let has_normals = md.normals.len() == n;
    if has_normals {
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, md.normals);
    }
    if let Some(uv) = md.uv_sets.first().filter(|u| u.len() == n) {
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv.clone());
    }
    if md.colors.len() == n {
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, md.colors);
    }
    let max = n as u32;
    let idx: Vec<u32> = md.triangles.iter().filter(|t| t.iter().all(|&i| i < max)).flatten().copied().collect();
    mesh.insert_indices(Indices::U32(idx));
    if !has_normals {
        mesh.compute_normals();
    }
    Some((mesh, tris))
}

/// Per-vertex joint indices and normalised weights (four influences per vertex).
type SkinWeights = (Vec<[u16; 4]>, Vec<[f32; 4]>);

/// Joint indices and weights per vertex from NiSkinData (top four influences).
fn skin_weights(skin_data: &Struct, vertices: usize) -> Option<SkinWeights> {
    let bones = skin_data.list("bone_list")?;
    let mut infl: Vec<Vec<(u16, f32)>> = vec![Vec::new(); vertices];
    for (b, bone) in bones.iter().enumerate() {
        let Value::Struct(bone) = bone else { continue };
        for w in bone.list("vertex_weights").unwrap_or_default() {
            let Value::Struct(w) = w else { continue };
            let (Some(i), Some(wt)) = (w.int("index"), w.float("weight")) else { continue };
            if let Some(v) = infl.get_mut(i as usize) {
                v.push((b as u16, wt));
            }
        }
    }
    if infl.iter().all(|v| v.is_empty()) {
        return None;
    }
    let mut joints = Vec::with_capacity(vertices);
    let mut weights = Vec::with_capacity(vertices);
    for mut v in infl {
        v.sort_by(|a, b| b.1.total_cmp(&a.1));
        v.truncate(4);
        let sum: f32 = v.iter().map(|x| x.1).sum();
        let mut j = [0u16; 4];
        let mut w = [0f32; 4];
        for (k, (b, wt)) in v.iter().enumerate() {
            j[k] = *b;
            w[k] = if sum > 0.0 { wt / sum } else { 0.0 };
        }
        if sum <= 0.0 {
            w[0] = 1.0;
        }
        joints.push(j);
        weights.push(w);
    }
    Some((joints, weights))
}

impl SpawnCtx<'_, '_> {
    /// Spawn the NIF at `path` under a new root entity. When `skeleton` is given, that
    /// file's node hierarchy is spawned first and skinned shapes bind to its bones by
    /// name: creature and character body meshes store their bones flat, as direct
    /// children of the root with bind transforms baked in, while animations are relative
    /// to the skeleton's hierarchy [observed, docs/spec-notes/nif.md §6].
    pub fn spawn_nif(&mut self, path: &str, skeleton: Option<&str>) -> Result<SpawnReport, String> {
        let root = self
            .commands
            .spawn((
                Name::new(path.to_string()),
                NifRoot,
                Transform::from_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
                Visibility::default(),
            ))
            .id();
        let skel = match skeleton {
            Some(sp) => Some(self.spawn_into(sp, root, None)?),
            None => None,
        };
        let mut report = self.spawn_into(path, root, skel.as_ref().map(|s| &s.nodes))?;
        if let Some(sk) = skel {
            // Animation targets: skeleton bones take precedence over the mesh's flat copies.
            for (k, v) in sk.nodes {
                report.nodes.insert(k, v);
            }
            report.skeleton = skeleton.map(str::to_string);
        }
        report.root = Some(root);
        Ok(report)
    }

    fn spawn_into(
        &mut self,
        path: &str,
        root: Entity,
        skeleton: Option<&HashMap<String, Entity>>,
    ) -> Result<SpawnReport, String> {
        let bytes = self.data.read(path).ok_or_else(|| format!("{path} not found"))?;
        let schema = self.data.schema.clone();
        let file = NifFile::parse(&bytes, Some(&schema)).map_err(|e| e.to_string())?;
        let dec = Decoded::new(&file, &bytes, &schema);
        let objects = scene_objects(&file, &dec);
        let mut report = SpawnReport { blocks: file.blocks.len(), ..Default::default() };

        // Pass 1: one entity per node or shape, with its local transform.
        let mut entity_of: HashMap<usize, Entity> = HashMap::new();
        let by_block: HashMap<usize, &SceneObject> = objects.iter().map(|o| (o.block, o)).collect();
        for o in &objects {
            let flags = dec.blocks[o.block].as_ref().ok().and_then(|s| s.int("flags")).unwrap_or(0);
            let mut e = self.commands.spawn((
                Name::new(o.name.clone().unwrap_or_else(|| o.type_name.clone())),
                to_bevy(&o.transform),
                Visibility::default(),
            ));
            if flags & 1 != 0 {
                e.insert((NifHidden, Visibility::Hidden));
            }
            let id = e.id();
            entity_of.insert(o.block, id);
            if let Some(n) = &o.name {
                report.nodes.entry(n.clone()).or_insert(id);
            }
        }
        for o in &objects {
            let parent = o.parent.and_then(|p| entity_of.get(&p)).copied().unwrap_or(root);
            self.commands.entity(parent).add_child(entity_of[&o.block]);
        }

        // World transforms in NIF space (for bounds), parents first.
        let mut world: HashMap<usize, Affine3A> = HashMap::new();
        fn world_of(b: usize, by_block: &HashMap<usize, &SceneObject>, cache: &mut HashMap<usize, Affine3A>, depth: usize) -> Affine3A {
            if let Some(w) = cache.get(&b) {
                return *w;
            }
            let o = by_block[&b];
            let local = affine(&o.transform);
            let w = match o.parent {
                Some(p) if depth < 512 && by_block.contains_key(&p) => world_of(p, by_block, cache, depth + 1) * local,
                _ => local,
            };
            cache.insert(b, w);
            w
        }
        let to_y_up = Affine3A::from_rotation_x(-FRAC_PI_2);

        // Pass 2: geometry and materials.
        for o in objects.iter().filter(|o| !o.is_node) {
            let Ok(shape) = &dec.blocks[o.block] else { continue };
            let Some(data_ref) = shape.block_ref("data") else { continue };
            let Some(data) = dec.get(data_ref) else { continue };
            let Some((mut mesh, tris)) = build_mesh(data) else { continue };

            // Properties: ancestors first, then the shape's own.
            let mut chain = vec![o.block];
            let mut cur = o.parent;
            while let Some(p) = cur {
                if chain.len() > 512 {
                    break;
                }
                chain.push(p);
                cur = by_block.get(&p).and_then(|x| x.parent);
            }
            let mut props = Props::default();
            for b in chain.iter().rev() {
                if let Some(refs) = dec.blocks[*b].as_ref().ok().and_then(|s| s.refs("properties")) {
                    for &r in refs {
                        apply_property(&file, &dec, r, &mut props);
                    }
                }
            }

            let formats = self.formats.as_deref();
            let base = props
                .textures
                .first()
                .filter(|t| !t.is_empty())
                .and_then(|t| self.textures.get(t, true, &self.data, &mut self.images, formats));
            let normal = props
                .textures
                .get(1)
                .filter(|t| !t.is_empty())
                .and_then(|t| self.textures.get(t, false, &self.data, &mut self.images, formats));
            if base.is_none() {
                if let Some(t) = props.textures.first().filter(|t| !t.is_empty()) {
                    report.textures_missing.push(t.clone());
                }
            }
            let has_uv = mesh.attribute(Mesh::ATTRIBUTE_UV_0).is_some();
            let normal = normal.filter(|_| has_uv && mesh.generate_tangents().is_ok());
            let alpha_mode = match props.alpha {
                // nif.xml AlphaFlags: bit 9 alpha test, bit 0 alpha blend.
                Some((flags, threshold)) if flags & 0x200 != 0 => AlphaMode::Mask(threshold as f32 / 255.0),
                Some((flags, _)) if flags & 1 != 0 => AlphaMode::Blend,
                _ => AlphaMode::Opaque,
            };
            let gloss = props.glossiness.unwrap_or(10.0);
            let material = StandardMaterial {
                base_color: Color::srgba(1.0, 1.0, 1.0, props.material_alpha.unwrap_or(1.0)),
                base_color_texture: base,
                normal_map_texture: normal,
                // Approximation until Layer 3 emulates the original shaders [hypothesis].
                perceptual_roughness: (1.0 - (gloss / 100.0).clamp(0.0, 0.9)).clamp(0.3, 1.0),
                metallic: 0.0,
                reflectance: 0.3,
                emissive: props.emissive.map(|e| LinearRgba::rgb(e[0], e[1], e[2])).unwrap_or(LinearRgba::BLACK),
                alpha_mode,
                double_sided: props.double_sided,
                cull_mode: if props.double_sided { None } else { Some(bevy::render::render_resource::Face::Back) },
                ..default()
            };

            // Skinning.
            let mut skinned = None;
            if let Some(si) = shape.block_ref("skin_instance").and_then(|r| dec.get(r)) {
                let bones = si.refs("bones").unwrap_or_default();
                let skin_data = si.block_ref("data").and_then(|r| dec.get(r));
                let joints: Option<Vec<Entity>> = bones
                    .iter()
                    .map(|&b| {
                        let b = usize::try_from(b).ok()?;
                        let by_name = by_block.get(&b).and_then(|o| o.name.as_ref()).and_then(|n| skeleton?.get(n));
                        by_name.or_else(|| entity_of.get(&b)).copied()
                    })
                    .collect();
                if let (Some(joints), Some(sd)) = (joints, skin_data) {
                    let vcount = mesh.count_vertices();
                    let binds: Option<Vec<Mat4>> = sd
                        .list("bone_list")
                        .unwrap_or_default()
                        .iter()
                        .map(|b| match b {
                            Value::Struct(b) => {
                                let t = b.strukt("skin_transform")?;
                                let nt = NifTransform::from_struct(t)?;
                                Some(Mat4::from(affine(&nt)))
                            }
                            _ => None,
                        })
                        .collect();
                    if let (Some(binds), Some((ji, jw))) = (binds, skin_weights(sd, vcount)) {
                        if binds.len() == joints.len() {
                            mesh.insert_attribute(Mesh::ATTRIBUTE_JOINT_INDEX, VertexAttributeValues::Uint16x4(ji));
                            mesh.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, jw);
                            skinned = Some(SkinnedMesh { inverse_bindposes: self.bindposes.add(binds), joints });
                        }
                    }
                }
            }

            // Bounds in Bevy world space.
            let w = to_y_up * world_of(o.block, &by_block, &mut world, 0);
            if let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
                for v in p {
                    let q = w.transform_point3(Vec3::from_array(*v));
                    report.bounds = Some(match report.bounds {
                        None => (q, q),
                        Some((lo, hi)) => (lo.min(q), hi.max(q)),
                    });
                }
            }

            let e = entity_of[&o.block];
            let mut cmd = self.commands.entity(e);
            match skinned {
                Some(s) => {
                    let mesh = mesh.with_generated_skinned_mesh_bounds().unwrap_or_else(|_| Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD));
                    cmd.insert((Mesh3d(self.meshes.add(mesh)), MeshMaterial3d(self.materials.add(material)), s, DynamicSkinnedMeshBounds));
                    report.skinned += 1;
                }
                None => {
                    cmd.insert((Mesh3d(self.meshes.add(mesh)), MeshMaterial3d(self.materials.add(material))));
                }
            }
            report.shapes += 1;
            report.triangles += tris;
        }
        Ok(report)
    }
}
