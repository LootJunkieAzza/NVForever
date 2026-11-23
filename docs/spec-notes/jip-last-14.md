# JIP LN NVSE, batch 14: the last JIP functions (2026-11-09, #1006)

Effects added to magic items, primitives, the auxiliary-variable condition,
IsSpellTargetList, DismemberLimb, FireWeaponEx, PlayIdleEx, PlaySound3DNode,
per-reference models, texture slots, VATS targets, the gun wobble, the projectiles' fuses
and weapons, GetSelfAsInventoryRef, animation sequence frequencies, and six nothing values
by decision, in `crates/fnv-game/src/nvse_jip14.rs`. Sources: the GECK wiki export, each
function's page [documented: the page named]; SetNoGunWobble's page is ToggleNoGunWobble.
The probe set `jip14` (`tools/capture/jip_probes.py`, lab `L5.ext.jip14`) asks the
original what the [hypothesis] values below leave open.

29 functions; JIP 102 → 73 on main; 0 with batches 12 (#1929) and 13 (#1931) merged too
[observed: the extender_census test, 2026-11-09: the 73 left are those two batches'].

## 1. Magic items and effects

| Function | NVForever |
|---|---|
| AddNewEffect `item effect duration magnitude [range] [area] [append]` | a new entry on an ingestible's, object effect's or actor effect's list, at the top or appended (1); the new count, -1 when not added [documented]; for the session [hypothesis]. Appending (1) a second copy of the stimpak's first effect returned 5 in the original where NVForever counts 6 [observed: jip14 addneweffect.append]: whether the entry was not added or the return differs is open [decision: NVForever adds it and returns the new count, as the page says]. A row asking it on Jet did not compile, in the original or in NVForever, and was dropped. Every Nth-effect function (GetNumEffects, Get/SetNthEffectBase, Get/SetNthEffectTraitNumeric, RemoveNthEffect and this count) indexes one list: AddNewEffect's top entries newest first, the record's entries RemoveNthEffect left, AddNewEffect's appended entries [decision: #2016, so a loop over 0 to GetNumEffects − 1 reads matching bases and traits]; an added entry's traits are its own, and RemoveNthEffect can take it away; applying the item applies it after the record's entries, which keep their places [decision: the active effects keep a record index]. The range is not kept [decision: NVForever's effect entries have none] |
| IsSpellTargetList `list` | any effect of the list's items on the actor [documented], as IsSpellTarget |

## 2. Primitives

| Function | NVForever |
|---|---|
| AddPrimitive `shape x y z` | 1 box, 2 sphere, 3 plane [documented]; on an activator (box or sphere) a trigger volume, on a CollisionMarker a collision shape [documented]; 1 when added. The activator's trigger takes it (`nvf_play::script_trigger`); a CollisionMarker's is kept but no collider is built [decision, follow-up] |
| HasPrimitive | a primitive from the record (XPRM) or a script [documented] |
| GetPrimitiveBound, SetPrimitiveBound `axis [value]` | the bound on the axis, half extents as the record keeps them [hypothesis]; -1 from the getter for a reference with no primitive [observed: jip14 primitivebound.doc]; the setter changes a trigger's volume from the next frame |

## 3. Actors

| Function | NVForever |
|---|---|
| AuxVarGetFltCond `quest index` | the float at index 0 of the calling reference's auxiliary variable named by the quest's string variable `index`; 0 otherwise [documented]; names with no owning mod's prefix [decision: a condition has no calling script to take a private name's mod from] |
| DismemberLimb `part [explode]` | the part 0-13 severed, or exploded (1) [documented], through the engine's dismemberment (`nvf_play::dismember::scripted_check`) |
| FireWeaponEx `weapon [node]` | FireWeapon's shot, from the node's place when the reference's 3D is known [documented] |
| PlayIdleEx `[idle]` | a parent: its first child whose conditions pass; a child: played whatever its conditions; none: PickIdle [documented] |
| PlaySound3DNode `sound node` | the sound at the node's place [documented]; at the reference while its 3D is not known yet [hypothesis] |
| SetNoGunWobble `[0/1]` | whether the actor's gun wobbles [documented: ToggleNoGunWobble]: 1 or no argument (the page's example) no wobble, 0 the wobble again [hypothesis: 1 is "no wobble"]; no wobble is no actor spread for its shots [hypothesis: the wobble is the aim's own unsteadiness, NVForever's actor spread] |
| SetVATSTargetable `0/1` | VATS does not mark such an actor (0), for the session [documented: "targetable again once a saved game is loaded"] |
| GetVATSTargets | the engine's VATS targets while VATS is up [documented]; NVForever's VATS marks under the crosshair and tells none yet, so no array [decision, follow-up] |

## 4. Models, textures and sequences

| Function | NVForever |
|---|---|
| SetRefrModelPath `[path]` | the reference's own model, under meshes; the base's without one [documented]; drawn at the reference's next drawing (`nvforever::destroyed`'s model overrides) [hypothesis: as after Update3D] |
| SwapTextureEx `ref block path slot` | slot 0, the diffuse map, as SwapTexture [documented]; other slots are not drawn [decision: NVForever's texture swaps change the diffuse map only] |
| GetBlockTextureSet `block` | no array [decision: NVForever's scene mirror keeps no texture sets; the page's answer for a block without one] |
| Get/SetAnimSequenceFrequency `sequence [f]` | a script's frequency, "*" every sequence [documented: GECK wiki SetAnimSequenceFrequency]; 1 when none set for a sequence the drawn model has [hypothesis]; -1 for a sequence it lacks [observed: jip14 seqfreq.doc]. The engine tells the game each drawn object's sequence names (`object_anims::publish_sequences`); an actor's animation files are not told, so an actor's unset sequence is -1 [decision]. Drawn object sequences (`GroupRig`) play at the active override frequency in `nvforever::object_anims` both on sequence start and live mid-sequence (with a zero-allocation fast path when no overrides exist, leaving unmodded vanilla playback unchanged); exact sequence names take precedence over `"*"` (case-insensitive) [hypothesis: exact-over-wildcard map precedence], `0.0` pauses playback without spuriously completing one-shot sequences [hypothesis: 0 frequency holds time], non-finite inputs are ignored, negative inputs clamp to `0.0`, and playback frequency is capped at `1e3` matching `nvf_assets::anim_play::MAX_FREQUENCY` [design]. Actor `.kf` sequence frequencies remain stored only [decision, follow-up] |
| UpdatePlayer3D | queues a transient one-shot full player 3D rebuild (`Game::take_update_player_3d`, not saved) and immediately refreshes the player's `AnimGroupOverride` folder entries [documented: UpdatePlayer3D; design]; `engine/nvforever`'s `player_body::ensure_body` rebuilds the third- and first-person rigs and equipped weapon models on the next frame (bypassing the `LOOK_REBUILD` slider throttle) and triggers `apply_anim_overrides` to clear cached clips |
| AttachExtraCamera `cameraName addOrRemove [parentNode]` | attaches (`addOrRemove != 0`) or detaches (`0`) a named session-transient extra camera anchored to `parentNode` (or the reference root when omitted/empty) on the calling reference; returns `1` on success, `0` on invalid name, unknown reference, detach of an absent name, or cap exceeded [documented: GECK wiki `AttachExtraCamera`; design: capped at 8 simultaneous cameras (`EXTRA_CAMERA_MAX`), cleared on save restore/new game] |
| ProjectExtraCamera `cameraName targetGeometry FoV [pixelSize] [grayscale]` | projects an attached named extra camera onto a 3D geometry node of the calling reference (or records a `*`-prefixed UI image tile path without requiring a calling reference), returning `1` on success and `0` when the camera is not attached, invalid, or outside the player's current interior cell / worldspace [documented: GECK wiki `ProjectExtraCamera`]. `pixelSize` defaults to `256` when omitted or `<= 0` [documented: GECK wiki] and is clamped to `16..=1024` [design]; `FoV` must be finite and `> 0` and is clamped to `0.5..=170.0` degrees [design]. `engine/nvforever`'s `extra_camera::ExtraCameraPlugin` spawns a bounded secondary Bevy `Camera3d` rendering the world and water layers (excluding `ARMS_LAYER` and `DECAL_LAYER` so PiP cameras do not require a per-camera `DepthPrepass`) into an `Rgba16Float` `Image` target, runs in `PostUpdate` before frustum, directional-shadow-cascade, and camera updates, safely deactivates on degenerate/zero-scale anchor transforms, binds onto a private cloned `NifMaterial` of `targetGeometry` with per-target original-material ownership across multi-camera contention, restores the original material on detach/cleanup, and does zero work when no extra cameras are registered [design]. UI tile projection (`*path`), `grayscale` shader desaturation, and live GPU visual scope verification remain follow-ups [hypothesis: NIF camera anchor looks along local +Y with +Z up, matching `WorldRoot`'s `game_to_bevy` basis] |

## 5. Projectiles and inventory references

| Function | NVForever |
|---|---|
| GetDetonationTimer | a timed explosive's seconds left; a mine or proximity charge not set off FLOAT_MAX; -1 for a projectile without a fuse [documented], as the engine tells them (`nvf_play::combat`); a fuse, and a new one SetDetonationTimer left, go with the projectile [decision: #2018] |
| SetDetonationTimer `seconds` | the projectile's fuse from now; a mine or proximity charge then goes off at the end [documented] |
| SetProjectileRefWeapon `weapon` | the weapon of the live projectile's record; returns it [documented]; what the impact takes from it is the engine's [hypothesis] |
| GetSelfAsInventoryRef | from an item's script in a container: the item as an inventory reference [documented]; elsewhere the call fails [observed: jip14 selfinvref, "script call failed"] |

## 6. Keyword condition and nothing values by decision

- `HasKeywordCond`: evaluates `Game::has_keyword` (shared with `HasKeyword` over
  `Jip2State::keywords`, case-insensitive, resolving references to base forms and
  `TempCloneForm` origins), accepting both `ref.HasKeywordCond "keyword"` and
  `HasKeywordCond form "keyword"` [documented: GECK wiki HasKeywordCond, HasKeyword;
  hypothesis: both calling conventions share `Game::has_keyword`].
- Nothing values by decision: AttachLine (line meshes; 0), FreePlayer (the page: "don't
  use this").

## 7. Follow-ups

- `AttachLine`.
- `ProjectExtraCamera` UI image tile projection (`*path`), `grayscale` shader output, and
  live GPU PiP scope visual verification.
- A CollisionMarker's primitive as a collider.
- GetVATSTargets from the engine's VATS.
- Actor `.kf` animation sequence frequency playback (drawn object sequences are played by `nvforever::object_anims`).
- Texture slots other than the diffuse map; GetBlockTextureSet.
- SetNoGunWobble's argument: the catalogue's parameter has no name and does not reach
  the function.
