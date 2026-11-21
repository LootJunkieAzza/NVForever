// [design] #2661 round 10b: one level of a mesh terrain blend colour atlas's mip chain, for
// the cells just baked (terrain_surface.rs `MipPipeline::record`). Level k of a cell is the
// linear mean of the 2 by 2 texels of level k - 1 under each of its texels. The atlas is
// three whole cells wide at every level (512, 1024 or 2048 texels per cell at level 0,
// halving down to one), so a cell's block at level k - 1 starts at twice its corner at
// level k and every 2 by 2 block lies inside its own cell: no cell's levels read another's.

// `slots[i].xy`: cell i's atlas column and row (top row 0); `count.x`: the number of cells.
struct Cells {
    slots: array<vec4<u32>, 9>,
    count: vec4<u32>,
}

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var destination: texture_storage_2d<rgba16float, write>;
@group(0) @binding(2) var<uniform> cells: Cells;

@compute @workgroup_size(8, 8, 1)
fn downsample(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.z >= min(cells.count.x, 9u) { return; }
    // Texels per cell side at this level.
    let side = textureDimensions(destination).x / 3u;
    if id.x >= side || id.y >= side { return; }
    let texel = cells.slots[id.z].xy * side + id.xy;
    let below = 2u * texel;
    let sum = textureLoad(source, below, 0) + textureLoad(source, below + vec2(1u, 0u), 0)
        + textureLoad(source, below + vec2(0u, 1u), 0) + textureLoad(source, below + vec2(1u, 1u), 0);
    textureStore(destination, texel, sum * 0.25);
}
