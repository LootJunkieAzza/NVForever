# Layer 2 design: NIF meshes, KF animations, DDS textures, asset viewer

Spec section 5 Layer 2 and milestone M2, in the section 9 format. Status 2026-10-07:
**formats complete and cross-validated on the owner's install (FNV 1.4.0.525)**; viewer
renders meshes with textures, skinning (bound to the creature's skeleton) and KF
animation (exit-criterion evidence in §4; the owner's visual check is the last step).

## 1. Summary and assumptions

Task 2: read every mesh, animation and texture the base game and DLC ship; prove it
against independent ground truth; render any mesh with its textures and play its
animations. Continued from Layer 1 in the same architecture (schema as data, tolerant
parsers, parity lab).

Decisions taken with the owner (2026-10-07):
- nv-rs (https://github.com/slaterain/nv-rs) is a source of **format facts only**: its
  2026-10-05 policy admits leaked Xbox 360 debug symbols and translated decompiled code,
  which our charter excludes. Its code is not copied. In practice nothing from it was
  needed: every layout came from nif.xml and was confirmed by the data.
- niftools nif.xml (GPL-3.0 repository) is used **as documentation**; layouts are
  re-expressed in our own JSON (as Layer 1 did with xEdit). No code is generated from it,
  so the licence choice (GPL vs MIT/Apache) stays open.
- Bevy 0.19.1 for the engine side (spec §7.1); see ADR-0001.

## 2. Findings and design

### 2.1 Crates

| Crate | Role | Bevy? |
|---|---|---|
| `fnv-nif` | container (`header`), schema loader (`schema`), expression language (`expr`), interpreter (`codec`), whole files (`file`), typed scene/geometry (`scene`), animation (`anim`) | no |
| `fnv-dds` | DDS header, format naming, face/mip layout with exact size check, RGBA8 for uncompressed layouts | no |
| `fnv-vfs` | read-only virtual file system: archives in precedence order + loose files | no |
| `fnvtool` | `nif coverage/decode/stats/scene/anim/textures`, `dds list` | no |
| `engine/nvf-viewer` + `engine/nvf-assets` (was `viewer/`) | Bevy app: ArchivePlugin, NIF spawning, textures, skinning, animation playback | yes (own workspace) |

### 2.2 NIF container (details and tags: `docs/spec-notes/nif.md` §1–§2)

- Versions present: 20.2.0.7 (user 11, Bethesda stream versions 14–34) in 25,754 files;
  20.0.0.4 (user 10/11) in 6 [observed].
- Block-size table (20.2.0.5+) makes every block skippable; it accounts for every byte in
  all 25,754 files that have it [observed]. Older files are split by decoding.
- Container rewrite is byte-identical for all 25,760 files [observed].

### 2.3 Schema-as-data for blocks (`schemas/nif/*.json`, format `fnv.nif.schema/1`)

- Our own format, documented at the top of `crates/fnv-nif/src/schema.rs`: primitives
  (including math compounds), enums/bitfields, structs with templates and arguments,
  blocks with single inheritance; fields gated by `since`/`until`, `cond` expressions,
  and `only`/`exclude` on the concrete block type; 1D, 2D and jagged arrays.
- The schema loader rejects unknown keys and types, inheritance cycles, and expressions
  naming anything but earlier fields or builtins.
- Five files, 160 block types plus their abstract parents, written by four parallel
  agents from nif.xml and verified only by the data:

| File | Content |
|---|---|
| `core.json` | object model, nodes, extra data, basic properties, triangle geometry, controller/interpolator bases |
| `scene.json` | shader properties, texture sets, skinning, bounds, special nodes, lights, cameras |
| `anim.json` | controllers, interpolators, key data, B-splines, sequences (incl. 20.0.0.4 layout) |
| `havok.json` | collision objects, rigid bodies, shapes, MOPP, constraints |
| `particles.json` | particle systems, modifiers, emitters, colliders, particle controllers |

**Coverage [observed]: 25,760 files, 1,036,695 blocks, 100% decode consuming exactly
their stored size and re-encode byte-identically; 0 unknown, 0 short, 0 errors.** This is
the strongest available check for field layouts; it cannot prove field *order* between
fields of equal size or the meaning of a value, which the scene/animation checks below
and the parity lab address.

Deviations from nif.xml found: emitters lack two documented floats; Bethesda 20.2
particle data has no per-vertex arrays; three 20.0.0.4 files use user version 10
(spec notes §4).

### 2.4 DDS (`docs/spec-notes/dds.md`)

26,907 textures: DXT1/3/5 plus A8R8G8B8, R8G8B8, A4R4G4B4; 6 cube maps; all power-of-two;
payload equals the mip-chain layout exactly in every file [observed].

### 2.5 Virtual file system and archive precedence

`fnv-vfs` indexes the archives lowest-precedence first and loose files last.
`SArchiveList` from the owner's `Fallout.ini` equals the shipped default [observed]:
Textures, Textures2, Meshes, Voices1, Sound, Misc. Then `Update.bsa`, then each plugin's
own archives in load order. Settled with in-game screenshots (2026-10-09, bsa.md
"Precedence") [observed]: `Update.bsa` overrides `Fallout - Meshes.bsa` and a DLC archive
overrides the base archives, whatever the file timestamps (the overriding archives are
the older files). Still **[hypothesis]**: `Update.bsa` against the DLC archives, DLC
against DLC, and loose files on top (no loose mesh or texture exists to test with).
`NVF_ARCHIVE_ORDER` (comma-separated, lowest first) replaces the order for diagnostics.

### 2.6 Scene and geometry conventions (spec notes §6)

Row-major matrices rotating column vectors, parent ∘ child composition, skin transform =
inverse bind matrix, counter-clockwise front faces — each measured on the data before the
viewer relied on it. Texture references: 98.1% of 137,908 resolve; the rest name files
absent from every archive.

### 2.7 Animation (spec notes §5)

`fnv_nif::anim` turns controller sequences into tracks that can be sampled at any time:
linear/quadratic/TBC/constant/Euler key groups, quaternion slerp, open uniform cubic
B-splines with compact control points (decompression and degree are hypotheses supported
by unit-length checks on ~1.6 M rotation samples). The viewer samples these tracks itself
(ADR-0001 §4).

### 2.8 Viewer (`engine/nvf-viewer`, ADR-0001, ADR-0002)

`nvf-viewer --data <Data> [--mesh path] [--anim file.kf] [--skeleton path] [--filter text]
[--screenshot out.png [--screenshot-at seconds]]`. Skinned meshes are bound to a skeleton:
`--skeleton`, else `skeleton.nif` in the mesh's folder or a parent folder (characters use
`characters\_male\skeleton.nif`). This is a browsing heuristic; in the game the creature or
NPC record names the skeleton (Layer 4). Spawns the scene graph (Z-up → Y-up at
the root), meshes with base and normal textures, alpha test/blend, double-sided stencil,
skinned meshes bound to their bone entities, and plays embedded sequences or those of a
`.kf`. Browsing keys and the headless screenshot mode are documented in
`engine/nvf-viewer/src/main.rs`. Materials approximate the original shaders (Layer 3 work).

## 3. Code, schema and tests

- Rust: `crates/fnv-nif`, `crates/fnv-dds`, `crates/fnv-vfs`, `crates/fnvtool/src/{nif,dds}_cmd.rs`,
  `engine/`. Tests: unit tests per module, `crates/fnv-nif/tests/synthetic.rs`
  (hand-built NIFs through the repository schemas, malformed-input properties), proptest
  on DDS headers.
- Independent oracles: `tools/capture/oracle_nif.py`, `oracle_dds.py` — written by a
  separate agent from the spec notes and CLI contract only (never seeing the Rust code or
  schemas), with synthetic tests.
- Parity lab: `parity-lab/tools/gen_layer2_scenarios.py` → `scenarios/layer2_nif.json`
  (stats, objects, shapes, skipped for 11 mesh archives) and `layer2_dds.json` (12 texture
  archives); HonestHearts is the holdout subject. **56/56 pass** on the owner's install.
- CLI contract: `docs/cli-contract.md` (Layer 2 section), `docs/spec-notes/dds.md` §5.

## 4. Validation plan and exit criterion (M2: "any mesh, texture, and animation renders correctly")

1. Container: rewrite byte-identical for every file — done (`fnvtool nif coverage`).
2. Blocks: exact consumption + byte-identical re-encode for every block — done (100%).
3. Independent oracle agreement on header stats, scene graph, geometry and texture
   layout for every archive — done (56/56).
4. Conventions measured on the data (spec notes §6) — done.
5. Texture references resolve through the VFS — 98.1% (remainder absent from the data).
6. Rendering [observed, headless `--screenshot` renders on the owner's GTX 1050]:
   `clutter\junk\nv\nvnukacolaquartz.nif` (alpha-blended glass, label readable: UVs and
   up axis right), `weapons\1handpistol\10mmpistolheavyframe.nif` (7 shapes, all
   textures), `creatures\deathclaw\deathclaw.nif` in bind pose (5 skinned shapes) and with
   `mtidle.kf` (68/68 tracks bound to `skeleton.nif`), `creatures\radroach\roach.nif` with
   `mtidle.kf` (37/37), `characters\_male\upperbody.nif` with the 20.0.0.4 `eatidle.kf`
   (27 of 50 tracks bound; the other 23 name nodes the FNV human skeleton lacks: tail
   bones, Torch, Quiver, BackWeapon, SideWeapon, thumb and twist bones [observed], which
   suggests a rig from an older game left in this 20.0.0.4 file [hypothesis]).
   Screenshots stay private (game-derived).
7. Visual comparison with the original game (replaces the owner's 10-minute check),
   partly done 2026-10-09 with the capture plugin's screenshots (no owner time): five
   fixed cameras (Nellis hangar, Camp Forlorn Hope guard tower, Westside Atomic Wrangler,
   two at the Prospector Saloon bar) captured in game and rendered by `nvforever` at the
   recorded poses. In the three exterior frames the placed meshes line up with the game's
   (position, orientation, texture placement; no flips or missing alpha seen on those
   objects) [observed]; the remaining differences are lighting and tone (renderer work).
   The two saloon frames (bottles, glass pitchers, cash register, broken mirror) were
   captured but not yet compared. `parity-lab/tools/gen_layer2_frames.py` turns these
   private frames (`<goldens>/frames_l2`) into `L2.frame.*` image scenarios; they are not
   yet run in the lab and their `min_ssim` (provisional 0.6) is to be fixed with the owner.

Known viewer limitations: the camera frames the bind-pose bounds, so an animation that
moves the root can sit off-centre (press F); materials approximate the original shaders;
environment maps, glow maps, particles and collision are not drawn.

## 5. Open questions and verification steps for the owner

1. ~~Visual check~~ — replaced by in-game screenshots (§4 item 7); the saloon frames are
   still to be compared.
2. ~~Renderer reference~~ — done by `capture_frames.py` (tools/capture/README.md §7).
3. ~~Archive precedence~~ — settled for `Update.bsa` and DLC archives over the base
   archives (bsa.md "Precedence"). Loose files stay untested: none exists in `Data` and
   NVForever never writes there; only the owner could add one by hand if wanted.
4. Licence decision (GPL-3.0 placeholder vs MIT/Apache) — unchanged from Layer 1; nothing
   in Layer 2 forces GPL.

## 6. Updated spec notes

`docs/spec-notes/nif.md` (container, object model, deviations, animation, verified
conventions), `docs/spec-notes/dds.md` (layout, formats, sizes, CLI), correction in
`docs/spec-notes/bsa.md` (archive flag 0x100 is set on the texture archives).
