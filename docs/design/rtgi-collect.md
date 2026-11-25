# Collecting a room for the bounce-light solver (#3009)

Part 6 of [the solver's plan](rtgi-solver.md): what the solver takes (meshes and textures as plain
data) out of the running game. All of it is home-grown, none of it a claim about the original game.
**Built: the reader**, which gets the arrays of a drawn mesh and the texels of a drawn texture that
light rays read. **Nothing in the game calls it**: no setting, no aid, no thread, and the loader does
what it did. The capture of a room's drawn objects and the putting together are still to come.

## Why the files are read again

A mesh's arrays and a texture's texels leave the main world when they go to the GPU
(`RenderAssetUsages::RENDER_WORLD`) [documented: `nvf-assets` `nif_spawn.rs` `build_mesh_desktop`,
`texture.rs` `checked_image_layers`]. [design] A worker reads the same files a second time through
the same mounted VFS; nothing is kept while loading, so a run that never asks pays nothing but four
bytes a prepared shape. The collision side reads the same models a second time already
(`nvf_assets::load_times`).

[design] **Assumption: files do not change while the game runs.** An archive is an open handle, a
loose file is opened again by its name. What can be compared is: a block's kind and name and the box
of its positions bit for bit; a texture's sides, levels and format. A loose file that changed and
kept all of these is not told from the one that was drawn.

## Which file it was

- `ModelCache::sources`: for a set of mesh ids the model's path (the cache key before its first `|`,
  the marks of the variants), the shape's block and its name. `PreparedShape` and `RigShape` keep the
  block's number. [design] It names a shape where an ordinal would not: the static and the rig's list
  differ, a skinned shape has a second mesh, hidden, helper, editor and rejected shapes are in
  neither, and the block is the same in every variant of a file.
- `TextureCache::sources`: for a set of image ids the resolved path, the usage and whether it is the
  clamped copy; the workers' entries too. A cube map is refused.
- [design] An id with two different answers is `None`, not either of them. No loader makes one.

## A model's shapes

`model::geometry::geometry` takes a file's bytes and the wanted blocks with their names and gives
each one's positions, normals, first texture coordinates, colours and indices, in the drawn mesh's
order: strips as triangles, triangles of missing corners left out, normals made where none are
stored, colours cleaned by the shader's vertex-alpha flag. They are the loader's own functions
(`shape_props`, `NumericRejections::admit`, `fix_vertex_colors`) on the same decode, so a change to
the loader changes both. It makes no handle, material, tangent or clip, and counts and logs nothing.

- [design] It does not ask whether the loader draws the shape: it is asked for drawn meshes.
- A skinned shape is its bind pose, as its static copy is drawn. Vertex morphs are the stored pose:
  those meshes stay on the CPU and the caller takes them from there.
- One wanted shape that is not there, has another name or draws nothing is the whole file's error,
  and so is a panic in a broken file: a lost wall lets light through.

## A texture's levels

`texture::levels::read` reads, parses and decodes only the levels that rays read, as RGBA8:
`nvf_rtgi::levels_read`, the rule `Sampler::new` picks its levels by: the first of at most 64 texels a
side for the colour, and the first of at most 256 for the alpha where **any** material with the
texture has a cutoff; else the last, however large.

| File | Decoded by |
|---|---|
| BC1 to BC3 that the GPU has as blocks | `texpresso` |
| BC1 to BC3 that the loader decoded (odd sides, no BC support) | `fnv_dds::decode_blocks`, as the loader |
| uncompressed | `fnv_dds::to_rgba8`, as the loader |
| BC7, BC4, BC5 | not (`Lost::Undecoded`) |

The colour space is the drawn image's, the wrap both of its sampler's axes. A texture that is not had
again says why (`Lost`: missing, too large, broken or not what was drawn, undecoded); whether it is
then white, grey or a reason to leave the room as it is is the caller's to say, which knows whether
an alpha test needs it.

## Limits

[design] Each is looked at before the work it bounds. They are ours.

| What | Limit | Where |
|---|---|---|
| a file as it is stored, and its contents | given by the caller | `Vfs::read_at_most`: an archived file's stored size before its buffer is made, the size its block declares before it is inflated (the inflater stops there); a loose file by the bytes it gives |
| a texture file, stored and decoded | 64 MiB each | `levels::MOST_BYTES` |
| a level that is decoded | 2,048 texels a side (16 MiB) | `levels::MOST_SIDE`; a file without smaller levels |

A read that is refused names the path and the number (`ErrorKind::FileTooLarge`). The reads count in
`fnv_vfs::read_stats` as all reads do. What a decoded NIF takes beside its bytes is not bounded by
more than the limit on those bytes, and not measured.

## Tests

`cargo test -p nvf-assets --test read_again` loads a made model through both of the loader's ways
(the main thread's stores and a worker's) and compares bit for bit, for every variant of the cache's
keys; and shows that reading again changes no store, counter or log line. Units: `model::geometry`,
`texture::levels`, `nvf_rtgi` `texture::tests::the_levels_read_are_enough`, fnv-vfs `at_most_tests`.
Not tested: a real data set, and the time and memory a room takes.
