# Patched dependencies

Crates copied from crates.io with a small change, used through `[patch.crates-io]` in
`engine/Cargo.toml`. Each keeps its own licence files. When the crate is upgraded, copy
the new version here and apply the change again (or drop the patch if upstream fixed it).

## gpu-allocator 0.28.0 (MIT OR Apache-2.0, Traverse-Research/gpu-allocator)

Used by wgpu for Vulkan memory. Change, in `src/vulkan/mod.rs` `MemoryType::free`: an
emptied general memory block is kept for reuse while fewer than
`KEEP_EMPTY_GENERAL_BLOCKS` (2) other empty general blocks of that memory type exist;
upstream destroys every emptied block except the last.

Still needed with Bevy 0.20.1 (#934): its wgpu-hal 30 uses gpu-allocator 0.28, and 0.28.0
is still the latest release, with the same free policy.

Why (#897): late in long sessions the block wgpu keeps fills with long-lived data, so each
frame's short-lived staging memory spills into a new block, which empties when the frame
is done and was destroyed at once. An Nsight Systems capture of the owner's game showed
about 285 blocks of 128 MiB made and freed a second, each zeroed and paged in by Windows,
and the render thread stalling 100-800 ms in the driver meanwhile.

Second change (#1429), in `src/vulkan/mod.rs` `MemoryBlock::new` and `src/lib.rs`: a
`vkAllocateMemory` that takes 1 ms or more logs `gpu memory: vkAllocateMemory of <n> MiB
(memory type <t>, host-visible|device-local, a general block|dedicated) took <ms> ms`, so a
single slow frame charged to a render system that only writes a buffer
(`upload_light_probes`, `write_binned_instance_buffers`) can be matched to a new memory
block. `std` is linked for the clock (wgpu-hal turns the crate's `std` feature off).

## bevy_render 0.20.1 (MIT OR Apache-2.0, bevyengine/bevy)

Why: in the owner's 5-player co-op stress test (2026-11-05) the AMD driver dropped the GPU
device (`Caught DeviceLost error: ... driver implementation is at fault`), and 40 ms later,
before Bevy's error handler had run, the game panicked: wgpu's `create_buffer_init` on a
storage buffer (`Failed to get mapped range for buffer created with mapped_at_creation`)
and the readback map callback (`gpu_readback.rs` `Failed to map buffer: BufferAsyncError`).
Every object of a lost device is invalid, so these maps fail; upstream unwraps them. The
changes, each marked `NVForever` in the source:

- `renderer/render_device.rs` `create_buffer_with_data`: wgpu's `create_buffer_init`
  written out, with a mapping that fails leaving the buffer unfilled (the first five
  logged) instead of panicking.
- `gpu_readback.rs` `map_buffers`, `view/window/screenshot.rs` `collect_screenshots`: a
  failed map or mapped range drops that readback or screenshot, logged.
- `slab_allocator.rs` (large mesh slabs), `storage.rs` (shader storage buffers),
  `diagnostic/internal.rs` (render diagnostics): a failed mapped range is skipped.
- `error_handler.rs`: the device-lost callback sets a flag, `device_was_lost()`, read by
  the game's render error handler and its panic hook (engine/nvforever/src/gpu_lost.rs),
  and keeps a second report instead of asserting there is none (a panic).

What the game then does (autosave, a message, a clean error exit): gpu_lost.rs. Drop the
patch when upstream Bevy handles map failures on a lost device without panicking.

## Dropped

- bevy_pbr 0.19.1 (#903, PR #905): the bindless index-table comparator in
  `MaterialBindlessSlab::get_bindless_index_table`, reversed in 0.19.1, which kept freed
  `ExtendedMaterial`s' textures in the slabs. Fixed upstream (bevyengine/bevy#25848);
  Bevy 0.20 has the fix, in `bevy_render::material_bind_groups` (#934).
