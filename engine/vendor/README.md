# Patched dependencies

Crates copied from crates.io with a small change, used through `[patch.crates-io]` in
`engine/Cargo.toml`. Each keeps its own licence files. When the crate is upgraded, copy
the new version here and apply the change again (or drop the patch if upstream fixed it).

## gpu-allocator 0.28.0 (MIT OR Apache-2.0, Traverse-Research/gpu-allocator)

Used by wgpu for Vulkan memory. Change, in `src/vulkan/mod.rs` `MemoryType::free`: an
emptied general memory block is kept for reuse while fewer than
`KEEP_EMPTY_GENERAL_BLOCKS` (2) other empty general blocks of that memory type exist;
upstream destroys every emptied block except the last.

Why (#897): late in long sessions the block wgpu keeps fills with long-lived data, so each
frame's short-lived staging memory spills into a new block, which empties when the frame
is done and was destroyed at once. An Nsight Systems capture of the owner's game showed
about 285 blocks of 128 MiB made and freed a second, each zeroed and paged in by Windows,
and the render thread stalling 100-800 ms in the driver meanwhile.

## bevy_pbr 0.19.1 (MIT OR Apache-2.0, bevyengine/bevy)

Change, in `src/material_bind_groups.rs` `MaterialBindlessSlab::get_bindless_index_table`:
the `binary_search_by` comparator gives the table's order against the index sought, as
upstream main does (bevyengine/bevy#25848); 0.19.1 gave it reversed. Drop
the patch on the upgrade to a Bevy with the fix.

Why (#903): with two index tables, as an `ExtendedMaterial` has (StandardMaterial's
`0..31` and `NifSurface`'s `50..54`), the search never found the first table, so freeing
a material never released its StandardMaterial textures and samplers from the slab. The
slabs kept every texture view ever bound, and so the GPU textures behind them [observed:
stress tour, one lap: slab resources 404 → 12,197 while NifMaterials fell from 6,951 to
5,901; two laps: GPU allocator textures 3,875 → 6,972 while the render world's
`GpuImage`s stayed near 3,350; the owner's 55-minute session ended in a wgpu
OutOfMemory].
