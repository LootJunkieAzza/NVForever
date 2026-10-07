# ADR-0002: Engine workspace and the first world renderer

- **Status:** Accepted (2026-10-07)
- **Context:** Layer 3 (spec §5) needs the asset code from the M2 viewer in a second
  binary that streams cells. ADR-0001 deferred the `AssetReader`/`AssetLoader` question to
  the start of Layer 3.

## Decisions

1. **One Bevy workspace, `engine/`** (was `viewer/`), still excluded from the root
   workspace for the reasons in ADR-0001 §2. Members: `nvf-assets` (library: archives,
   NIF spawning, prepared models, textures, KF playback), `nvf-viewer` (the M2 viewer,
   unchanged behaviour) and `nvforever` (the game binary; a free-camera explorer at M3).
   Bevy stays pinned at `=0.19.1` in `engine/Cargo.toml`.
2. **Static references use prepared models** (`nvf_assets::model`): each NIF is parsed
   once into (mesh, material, model-space transform) per visible shape, and every
   reference spawns one entity per shape sharing those handles, so Bevy batches them.
   The full scene-graph spawner remains for animated and skinned objects.
3. **Still no `AssetLoader`.** World content is found through records (a cell's LAND, a
   base object's model), not by path alone, and terrain meshes are built from record data.
   Cells are loaded synchronously, one per frame, nearest first (49 Goodsprings cells
   take 2.6 s in total on the owner's machine). Next step when needed: decode records and
   NIFs on `AsyncComputeTaskPool` and only create assets on the main thread. Rejected for
   now: an `AssetLoader` per format. Revisit when loading stalls become visible.
4. **Terrain is drawn in multiple passes with the standard material** (interim): per
   LAND quadrant, an opaque mesh with the base texture and one alpha-blended copy per
   additional layer with the layer's opacity in vertex alpha, ordered by depth bias.
   Lighting and fog match the objects for free. It will be replaced by a single-pass
   splatting material once the layer rules are confirmed against in-game screenshots.
   Rejected: baking layers into per-quadrant textures on the CPU (needs a BC decoder and
   loses resolution).
5. **Coordinates:** one root entity maps game space (Z up) to Bevy space (Y up); cells and
   references keep game units and positions underneath it. Worldspace coordinates reach
   ±270,000 units, where f32 resolution is still about 0.03 units.

## Consequences

- CI builds and lints the `engine/` workspace on all three platforms (`viewer` job).
- Paths in docs and commands changed from `viewer/` to `engine/`.
