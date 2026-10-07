# ADR-0001: Bevy version and how Layer 2 assets reach Bevy

- **Status:** Accepted (2026-10-07); paths and decision 3 revisited in ADR-0002 (the
  viewer now lives in the `engine/` workspace)
- **Context:** spec §7.1 asks for one pinned Bevy version, format crates without Bevy,
  and a recorded decision (with rejected alternatives) for every engine-side choice.

## Decisions

1. **Bevy 0.19.1, pinned with `=`** in `viewer/Cargo.toml`. Latest stable on 2026-10-07
   (0.20 is at rc.2). APIs were checked against the 0.19.1 sources and shipped examples,
   not from memory. Upgrades happen in their own commit with a full parity-lab run.
2. **The Bevy app is its own Cargo workspace** (`viewer/`, excluded from the root
   workspace). The format crates, `fnvtool` and their CI never compile Bevy (a cold Bevy
   build takes several minutes). Rejected: one workspace with a feature flag — every
   `cargo test --workspace` would still resolve Bevy.
3. **Assets are converted by direct spawning for now** (`viewer/src/nif_spawn.rs`: read
   through `fnv-vfs`, decode with `fnv-nif`, add `Mesh`/`StandardMaterial`/`Image`
   assets). Rejected for the moment: a Bevy `AssetReader` + `AssetLoader` pair. It is the
   target design from spec §7.1 and fits streaming in Layer 3, but asynchronous loading
   adds nothing to a single-mesh viewer and would double the API surface to verify.
   Revisit at the start of Layer 3 (cell streaming).
4. **Animation is sampled by our own system** from `fnv_nif::anim` (B-splines, TBC keys,
   cycle types) and written to `Transform`s, instead of converting to Bevy
   `AnimationClip`s. Keeps the original interpolation rules in the engine-independent
   crate and avoids resampling error. Rejected: baking to Bevy curves at a fixed rate
   (lossy, and Bevy's animation API has changed in most recent releases).
5. **Textures are uploaded as BC1/BC2/BC3 directly** with their stored mip chains;
   only the three uncompressed layouts are converted on the CPU. Bevy requests every
   adapter feature by default, so desktop GPUs expose BC; the viewer checks
   `CompressedImageFormatSupport` and reports textures it cannot upload. Rejected: CPU
   decompression of everything (memory ×4–8, no benefit on desktop).
6. **Materials use `StandardMaterial` as an approximation.** The original shading intent
   (BSShader* properties, glow, environment maps, specular-in-normal-alpha) is Layer 3
   work and will need a custom material; the viewer marks this as a hypothesis-level
   approximation.

## Third-party crates added in Layer 2

| Crate | Use | Licence | Notes |
|---|---|---|---|
| `bevy` 0.19.1 | viewer only | MIT OR Apache-2.0 | default features |
| `clap` 4 | viewer CLI | MIT OR Apache-2.0 | already used by `fnvtool` |

No new crates in the format layer: the NIF interpreter, DDS parser and VFS use only
existing dependencies. `texture2ddecoder`/`bcdec_rs` (CPU BCn decode) were evaluated
and deferred until something needs CPU-side pixels (e.g. image comparison in the lab).
