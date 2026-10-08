//! Distant LOD: the game's pre-built terrain LOD chunks and object LOD blocks drawn
//! beyond the loaded cells (docs/spec-notes/world.md §7, docs/design/layer3.md).
//!
//! Observed layout: `meshes\landscape\lod\<world>\<world>.level4.x<X>.y<Y>.nif` covers
//! the 4 × 4 cells from (X, Y) (X, Y multiples of 4), its root node translated to the
//! chunk's south-west corner, with its own diffuse and normal textures; object LOD
//! blocks are `...\blocks\<world>.level4.x<X>.y<Y>.nif` over the same squares.
//! Coarser levels (8, 16, 32) exist and are not used yet.
//!
//! Interim rules [hypothesis, to compare with screenshots]: terrain chunks are drawn with
//! a negative depth bias so loaded terrain wins where both exist, and where a chunk
//! overlaps the loaded cells only its triangles reaching outside them are kept, with the
//! vertices inside sunk below the loaded terrain; object
//! blocks are drawn only where no cell of their square is loaded.

use crate::world::{bevy_to_game, Streamer, WorldData, WorldRoot};
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use fnv_world::{cell_of, CELL_SIZE};
use nvf_assets::model::ModelCtx;
use std::collections::HashMap;

const LEVEL: i32 = 4;

type Grid = (i32, i32);
type BuiltFor = (Grid, Grid, Grid);

#[derive(Resource)]
pub struct LodState {
    /// Chunks drawn within this many chunks of the camera's chunk.
    pub radius: i32,
    pub enabled: bool,
    world_name: String,
    /// The loaded square (min cell, max cell) and centre chunk the current LOD entities
    /// were built for.
    built_for: Option<BuiltFor>,
    entities: HashMap<(i32, i32), Vec<Entity>>,
    pub chunks: usize,
    pub blocks: usize,
}

impl LodState {
    pub fn new(world: &WorldData, radius: i32, enabled: bool) -> LodState {
        let name = world.index.worldspaces[&world.world].edid.clone().unwrap_or_default().to_ascii_lowercase();
        LodState { radius, enabled, world_name: name, built_for: None, entities: HashMap::new(), chunks: 0, blocks: 0 }
    }

    /// Turn distant LOD on or off (off: its entities are removed now).
    pub fn set_enabled(&mut self, commands: &mut Commands, enabled: bool) {
        self.enabled = enabled;
        self.built_for = None;
        for (_, es) in self.entities.drain() {
            for e in es {
                commands.entity(e).despawn();
            }
        }
        self.chunks = 0;
        self.blocks = 0;
    }

    fn terrain_path(&self, x: i32, y: i32) -> String {
        format!("meshes\\landscape\\lod\\{0}\\{0}.level{LEVEL}.x{x}.y{y}.nif", self.world_name)
    }

    fn block_path(&self, x: i32, y: i32) -> String {
        format!("meshes\\landscape\\lod\\{0}\\blocks\\{0}.level{LEVEL}.x{x}.y{y}.nif", self.world_name)
    }
}

/// Inside the loaded square of cells (inclusive), in game x/y.
fn in_square(p: Vec3, lo: (i32, i32), hi: (i32, i32)) -> bool {
    let (x0, y0) = (lo.0 as f32 * CELL_SIZE, lo.1 as f32 * CELL_SIZE);
    let (x1, y1) = ((hi.0 + 1) as f32 * CELL_SIZE, (hi.1 + 1) as f32 * CELL_SIZE);
    p.x > x0 && p.x < x1 && p.y > y0 && p.y < y1
}

/// Depth by which LOD vertices inside the loaded square are sunk (game units).
const SINK: f32 = 2000.0;

/// A copy of `mesh` without the triangles that lie entirely inside the loaded square;
/// vertices inside it are sunk by [`SINK`] so the triangles crossing its edge slope
/// down under the loaded terrain instead of poking through it.
fn trim_mesh(mesh: &Mesh, to_world: &bevy::math::Affine3A, lo: (i32, i32), hi: (i32, i32)) -> Option<Mesh> {
    let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { return None };
    let from_world = to_world.inverse();
    let mut inside = Vec::with_capacity(pos.len());
    let mut new_pos = Vec::with_capacity(pos.len());
    for p in pos {
        let w = to_world.transform_point3(Vec3::from_array(*p));
        let i = in_square(w, lo, hi);
        inside.push(i);
        let w = if i { w - Vec3::Z * SINK } else { w };
        new_pos.push(from_world.transform_point3(w).to_array());
    }
    let idx: Vec<u32> = match mesh.indices()? {
        Indices::U32(i) => i.clone(),
        Indices::U16(i) => i.iter().map(|&v| v as u32).collect(),
    };
    let kept: Vec<u32> = idx.chunks(3).filter(|t| t.iter().any(|&v| !inside[v as usize])).flatten().copied().collect();
    if kept.is_empty() {
        return None;
    }
    let mut out = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    for (id, values) in mesh.attributes() {
        out.insert_attribute(*id, values.clone());
    }
    out.insert_attribute(Mesh::ATTRIBUTE_POSITION, new_pos);
    out.insert_indices(Indices::U32(kept));
    Some(out)
}

pub fn update_lod(
    mut commands: Commands,
    streamer: Res<Streamer>,
    mut lod: ResMut<LodState>,
    root: Query<Entity, With<WorldRoot>>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    mut models: ModelCtx,
) {
    if !lod.enabled {
        return;
    }
    let (Ok(root), Ok(cam)) = (root.single(), camera.single()) else { return };
    let g = bevy_to_game(cam.translation());
    let c = cell_of(g.x, g.y);
    let r = streamer.radius;
    let (lo, hi) = ((c.0 - r, c.1 - r), (c.0 + r, c.1 + r));
    let centre_chunk = (c.0.div_euclid(LEVEL), c.1.div_euclid(LEVEL));
    if lod.built_for == Some((lo, hi, centre_chunk)) {
        return;
    }
    lod.built_for = Some((lo, hi, centre_chunk));
    for (_, es) in lod.entities.drain() {
        for e in es {
            commands.entity(e).despawn();
        }
    }
    lod.chunks = 0;
    lod.blocks = 0;
    let lr = lod.radius;
    for dx in -lr..=lr {
        for dy in -lr..=lr {
            let (cx, cy) = ((centre_chunk.0 + dx) * LEVEL, (centre_chunk.1 + dy) * LEVEL);
            let overlaps = cx <= hi.0 && cx + LEVEL > lo.0 && cy <= hi.1 && cy + LEVEL > lo.1;
            let mut spawned = Vec::new();
            let (tp, bp) = (lod.terrain_path(cx, cy), lod.block_path(cx, cy));
            // Chunks exist only inside the worldspace's LOD grid; skip the rest quietly.
            let terrain = models.data.vfs.exists(&tp).then(|| models.model_with_cpu(&tp)).flatten();
            if let Some(model) = terrain {
                for s in &model.shapes {
                    let st = model.root * s.transform;
                    let mesh = if overlaps {
                        // The GPU mesh cannot be read back once uploaded; trim the CPU copy.
                        let Some(m) = s.cpu.as_deref() else { continue };
                        let Some(t) = trim_mesh(m, &st, lo, hi) else { continue };
                        models.meshes.add(t)
                    } else {
                        s.mesh.clone()
                    };
                    // Push LOD terrain behind loaded terrain where both exist.
                    if let Some(mut mat) = models.materials.get_mut(&s.material) {
                        if mat.depth_bias == 0.0 {
                            mat.depth_bias = -50.0;
                        }
                    }
                    let e = commands
                        .spawn((Mesh3d(mesh), MeshMaterial3d(s.material.clone()), Transform::from_matrix(Mat4::from(st))))
                        .id();
                    commands.entity(root).add_child(e);
                    spawned.push(e);
                }
                lod.chunks += 1;
            }
            if !overlaps {
                let block = models.data.vfs.exists(&bp).then(|| models.model(&bp)).flatten();
                if let Some(model) = block {
                    for s in &model.shapes {
                        let st = model.root * s.transform;
                        let e = commands
                            .spawn((Mesh3d(s.mesh.clone()), MeshMaterial3d(s.material.clone()), Transform::from_matrix(Mat4::from(st))))
                            .id();
                        commands.entity(root).add_child(e);
                        spawned.push(e);
                    }
                    lod.blocks += 1;
                }
            }
            lod.entities.insert((cx, cy), spawned);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_test() {
        let (lo, hi) = ((-2, -2), (2, 2));
        assert!(in_square(Vec3::new(0.0, 0.0, 0.0), lo, hi));
        assert!(in_square(Vec3::new(3.0 * CELL_SIZE - 1.0, -2.0 * CELL_SIZE + 1.0, 0.0), lo, hi));
        assert!(!in_square(Vec3::new(3.0 * CELL_SIZE + 1.0, 0.0, 0.0), lo, hi));
    }
}
