# JohnnyGuitar NVSE: the NIF, particle and camera functions (2026-11-09, #1007)

21 JohnnyGuitar functions, a slice the coordinator gave this builder beside the
JohnnyGuitar batches (GetNiLODLevel, SetNiLODLevel, SetCameraShake, the viewmodel clip
distance and StopVATSCam were built already in batch 7; GetPlayerCamFOV is batch 9's, #1930). Code:
`crates/fnv-game/src/nvse_jg_nif.rs`, the scene mirror's node info and particle edits in
`crates/fnv-game/src/nif_blocks.rs`, the camera's state in
`crates/fnv-game/src/functions/camera.rs`; the engine's side in
`engine/nvforever/src/nif_mirror.rs` (bounds, particle systems, alpha and stencil
properties), `engine/nvforever/src/particles.rs` (the live systems' values),
`engine/nvf-play/src/script_camera.rs` (the camera's overrides and place) and
`engine/nvf-play/src/combat.rs` (the hit points). Tests: `crates/fnv-game/tests/jg_nif.rs`
and the unit test of `nvse_jg_nif.rs`.

Source: the GECK wiki export [documented: the page named], in our own words. Five pages
are templates with nothing filled in (Get/SetAlphaPropertyValue,
Get/SetStencilPropertyValue, IsNiSequenceActive, UpdateScenegraph): their arguments are
the pages' and their meaning a [hypothesis]. Not probed yet: set `jgnif` of
`tools/capture/jip_probes.py` (lab `L5.ext.jgnif`), questions in §4.

## 1. Nodes and particles

| Function | NVForever |
|---|---|
| Get3DDistanceBetweenNiNodes `ref1 ref2 node1 node2` | the distance between the two nodes in the world, the references' places and the nodes above included (`nif_blocks::node_world`, as GetNifBlockTranslationAlt's world place) [documented]; 0 without either [hypothesis] |
| Get3DDistanceToNiNode `node x y z` | from the node to the point [documented] |
| Get3DDistanceFromHitToNiNode `node` | from where the last hit on the calling reference struck: the engine tells each shot's and blow's point (`Game::note_hit_point`) [documented: in OnHit or the hit's event]; 0 before any [hypothesis] |
| GetNiBound `[block] [pc]` | [x, y, z, radius] in the world, the root's without a block [documented], and for a block the reference does not have [hypothesis: §4 bound gave four values]: the sphere round the boxes of the shapes under the node, the node's own place with radius 0 when it has none [hypothesis: the engine's boxes for the file's bounds]; an empty array before the engine told the tree [hypothesis] |
| GetNiPSysModifierValue, SetNiPSysModifierValue `block id [value] [pc]` | the particle system of that name, or emitting from a node of that name, of a placed model: its values by the page's IDs 0-19 [documented: the table]; 0 order (not kept), 1 active, 16 the particles' base scale [hypothesis]; the setter changes the engine's live system while it lives, 1 when it found one [documented: "success:bool"] |
| GetParticleEmitterSpawnRate, SetParticleEmitterSpawnRate `block [value] [pc]` | the system's birth rate per second; set, it replaces any keyed rate [hypothesis] |
| Get/SetAlphaPropertyValue `block item [value] [pc]` | the shape's NiAlphaProperty from the model file: item 0 the flags, 1 the threshold [hypothesis: the page is a template; the fields' order, nif.xml]; set for the session's reads; the drawn shape keeps its own [not built: engine follow-up] |
| Get/SetStencilPropertyValue `block item [value] [pc]` | NiStencilProperty: 0 the flags, 1 the reference, 2 the mask [hypothesis], the same way |
| SetSwitchNodeIndex `block index [pc]` | the node's child of that index shown alone, the others hidden through the NIF block flags [documented: "Sets the active index"]; an index past them hides all [hypothesis] |
| IsNiSequenceActive `sequence [block] [pc]` | 1 when the reference plays a sequence of that name: its body's clip or one a script played (by file name), or its movement group [hypothesis: the page is a template] |
| GetSequenceAnimGroup `slot` | the anim group code in that slot, 0 idle … 7 special idle [documented]: Idle (0) in slot 0 standing [observed: §4], the body's other movement groups in slot 1; -1 otherwise [observed: slot 4; hypothesis: the engine tells the movement group alone, and an actor it told nothing of stands] |
| UpdateScenegraph `type [time] [controllers] [block] [pc]` | nothing to do [decision: the engine updates every drawn node each frame]; 1 on a reference [observed: §4 refresh] |
| Update3DAlt `flags` | 1 model, 2 skin, 4 head, 8 face: the actor drawn again through the appearance changes (`crate::appearance`); 16 scale, 32 lights, 64 position need no request, the engine takes them each frame [documented: the flag table; decision]; 1 [observed: §4 refresh] |

The first-person model (pc) is not read: the mirror has the drawn model only, as for the
JIP block functions.

## 2. The camera

| Function | NVForever |
|---|---|
| GetCameraTranslation `x y z worldOrLocal` | the VM writes the three: the camera's place in the world, or (1) relative to the player in the player's frame, x right, y ahead, z up [documented: "local or world"; hypothesis: the local frame]; 0 0 0 without an engine |
| SetCameraTranslate `override x y z` | the third-person camera put at that place relative to the player (GetCameraTranslation 1's frame), or the override cleared [documented]; the engine applies it after placing the camera (`nvf_play::script_camera`), not to the fly camera |
| SetCameraRotate `override axis angle [ref]` | the third-person camera's angle about 1 X (down positive, as the game's angles), 2 Y, 3 Z in degrees, or cleared; with a reference it keeps looking at it [documented]; clearing an axis clears the tracking [hypothesis] |
| WorldToScreen `xOut yOut zOut x y z handleType [ref]` | the VM writes X and Y (0 to 1, left to right, top to bottom) and the distance ahead; 1 when on the screen; off it, 0 on the square's edge, 1 on the circle, 2 as projected [documented: the table]; with a reference the point is an offset from its place [documented] |

The engine tells the camera each frame (`CameraRequests::view`): place, heading, pitch,
roll, vertical field of view and the width over the height (GetPlayerCamFOV, JohnnyGuitar batch 9, may read it).

## 3. Engine follow-ups

The alpha and stencil properties drawn as set; the first-person model's nodes (pc); the
other sequence slots of GetSequenceAnimGroup.

## 4. Probe answers (set jgnif, run 2026-11-09)

On the player at Doc Mitchell's, in the original game with the extenders [observed], and
what NVForever does with each:

| Probe | Original | NVForever |
|---|---|---|
| `between`: head to pelvis above 0, a node to itself | `1 0` | the engine's node places; the records-only lab candidate has no 3D and gives `0 0` |
| `tonode`: the head 80-160 from the player's place, a missing block | `1 0` | as `between` |
| `fromhit`: no hit yet | `0` | `0` |
| `bound`: the root's size and radius above 0, a missing block's size | `4 1 4` | a missing block gives the root's bound [hypothesis from the 4]; in the candidate empty |
| `missing`: a missing block's particle and property values | `0 0 0 0` | `0 0 0 0` |
| `camera`: the camera within 200 of the player, the local place's height above 50 | `1 0` | the engine's camera; the candidate has none |
| `screen`: WorldToScreen into variables | did not compile | not asked any more |
| `sequences`: slot 0, slot 4, `mtidle` standing | `0 -1 0` | Idle (0) in slot 0 of an actor standing or told nothing of, as here |
| `refresh`: UpdateScenegraph 0, Update3DAlt 32 | `1 1` | both 1, as here |

The local camera height not above 50 leaves the local frame open: the place may be the
camera's against the player's eyes, not the feet [hypothesis].
