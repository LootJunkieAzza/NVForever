# SUP NVSE batch 7: NIF block interpolations, vertices, HUD bar sources (2026-11-08, #1008)

12 SUP functions: the NIF block interpolations (8), the vertices of a NIF block (2) and
the HUD bars' script-variable and UDF values (2). Code: `crates/fnv-game/src/nvse_sup7.rs`
(the functions and `Game::sup7`), the scene mirror's helpers in
`crates/fnv-game/src/nif_blocks.rs` (`push_edit`, `node_now`, `vertices`), the bars'
`bar_id` and `set_value` in `crates/fnv-game/src/ui_hudbar.rs`, the VM's hand-over of a
variable by place (`fnv_script::vm::Host::var_place`) and the engine's side of the
vertices in `engine/nvforever/src/nif_mirror.rs`. Tests: `crates/fnv-game/tests/sup7.rs`
(the engine's part played by the test) and the unit test of `nvse_sup7.rs`.

Source: the GECK wiki export [documented: the page named], in our own words. The modes'
category page ("NifBlock Interpolation Functions (SUP)") is not in the export; the modes
are read from the functions' examples. Not probed yet: set `sup7` of
`tools/capture/jip_probes.py` (lab `L5.ext.sup7`), questions in §5.

## 1. The interpolations

| Function | NVForever |
|---|---|
| NifBlockInterpolateTranslation `block x0 y0 z0 x1 y1 z1 timer mode world pc calcStart` | the block's translation from start to end in `timer` seconds by mode [documented]; 1 when it runs, 0 without a reference or a block name [hypothesis] |
| NifBlockInterpolateRotation `block x0 y0 z0 x1 y1 z1 timer mode pc calcStart` | the rotation (degrees, as GetNifBlockRotation) the same way [documented] |
| NifBlockInterpolateScale `block s0 s1 timer mode pc calcStart` | the scale the same way [documented] |
| NifBlockInterpolateStop `[block] [type] [pc]` | stops the matching ones; 1 when one matched [hypothesis: "success:bool"]; the block keeps the value it had [hypothesis] |
| IsNifBlockInterpolated `[block] [type] [pc]` | 1 when one matches [documented] |
| NifBlockInterpolateSetTrait `trait value [block] [type] [pc]` | trait 0: the start value set again when it ends [documented]; 1 when one matched, 0 for another trait [hypothesis] |
| SetSUPOnInterpolateFinishEvent `udf [loops]` | the handler, called `{ref, block, type, fromLoop, pc}` when one ends, and with `loops` at each loop's end too [documented: the handler skeleton]; the same handler again changes its loops flag [hypothesis] |
| RemoveSUPOnInterpolateFinishEvent `udf` | takes the handler away [documented] |

- **Modes** [hypothesis: the category page is missing]: 0 start to end once (the first
  scale example, "from start to end in 2 seconds"); 1 start to end and back to start,
  once ("from start to end and to start again"); 2 there and back again and again ("Infinite
  amount of times … and then back"); 3 start to end again and again, the remaining case.
  Another number is clamped to 0-3.
- **Types**: 1 is the rotation [documented: NifBlockInterpolateStop's example]; 0 the
  translation and 2 the scale [hypothesis]; -1 any [documented: "if type -1 specified - it
  will be ignored"].
- **Searches**: no block, every interpolation of the reference [documented: "Will stop all
  interpolations on Player"]; a block without pc is the third-person model's [documented:
  "third person by default"].
- **One of each type a block** [hypothesis: the pages allow "many types of interpolations at
  the same time"]: a new one of the same type on the same block and model replaces the old.
- **calcStart** takes the block's value from the scene mirror as the start [documented:
  "Scale N will be calculated automatically"]; without the mirror's tree yet (the first
  frame a reference is asked about) the given start is used [hypothesis].
- **The world flag** of the translation is not used [decision: the mirror holds and sets
  values in the parent's space, as SetNifBlockTranslation; engine follow-up: a world
  translation turned into the parent's space].
- **The first-person model** (pc, player only): NVForever's mirror has one tree a
  reference, the drawn one, so a first-person interpolation changes that tree [decision:
  kept apart for the searches and the events; engine follow-up: the first-person
  viewmodel's nodes in the mirror].
- **Running**: each frame (`Game::tick`, before the frame's time step) the value at the
  game time since the start goes to the engine as the NIF block setters' `SceneEdit`, and
  shows in the mirror at once; the engine holds it over the animation as it holds
  SetNifBlockScale's (`nif_mirror.rs`). Menu mode and FreezeTime hold the game time, so
  they hold the interpolation [hypothesis]. The finish event runs after the frame's values
  are sent, so a handler may start a new one on the block [documented: "It is possible to
  set a new interpolation for this block in the event iself"].
- **Not saved** [hypothesis: SUP's pages say a hot reload loses the bars' variables; the
  interpolations likewise run in the session only].

## 2. The vertices

| Function | NVForever |
|---|---|
| GetNifBlockNumVerticles `block [pc]` | the number of the shape's vertices [documented] |
| NifBlockGetVerticles `block [pc]` | three arrays, X, Y and Z of each vertex [documented: `myarray[1][0]` the first vertex's Y] |

The engine tells the vertices of the shapes a script asked for, each frame from then on,
in the shape's own space as its mesh holds them (`SceneMirror::set_vertices`). Asked the
first time they come with the next frame, as the JIP block functions' trees do; until then,
and for a node or a missing block, 0 and an empty array [hypothesis]. A mesh kept only on
the GPU tells none, and a skinned shape's are its bind pose [hypothesis; engine follow-up
if a mod needs a posed one]. pc is not read (the drawn model only).

## 3. The HUD bars' sources

| Function | NVForever |
|---|---|
| HudBarSetValueScriptVar `bar variable calcMax` | the bar reads the variable [documented] |
| HudBarSetValueUDF `bar udf [calcMax] [caller]` | the bar reads the UDF's result, called on the caller when given [documented] |

- The variable goes to the game by its place, not its value: the VM hands a global,
  another script's variable or a local of a script instance over as a number
  (`Host::var_place`, `-1 - n`), which the game reads again later. A value that is no
  such variable (a number, a result script's local) is refused [hypothesis].
- Read each fUpdateTimer seconds (bar trait 2, 0.1 s by default) [documented: "Value will be
  updated each instance of Bar update"], the first time in the frame the source was set
  [hypothesis]. A value that is no number is not written [design].
- calcMax 1: the value against the bar's max as HudBarSetValueFloat [documented]; 0: "raw
  value will be passed", read as the meter's share 0.00-1.00 as HudBarSetValuePercentage,
  the value text showing the raw value [hypothesis].
- 1 when set; -1 for a missing bar or a refused source, as the other value setters
  [hypothesis: "success:int"]. A new source replaces the bar's old one; a destroyed bar
  takes its source with it [hypothesis].

## 4. Left as nothing values

None. The engine follow-ups: the translation's world flag and the first-person model (§1),
posed vertices of skinned shapes (§2).

## 5. Probe questions (set sup7, not run)

On the player's third-person "Bip01 Head"; every interpolation stopped, the scale set back
to 1, the handler removed, the bar destroyed.

1. `scale.start`: the call's result and IsNifBlockInterpolated with the block, type 2,
   type 1, `-1 1` and no arguments (hypothesis `1 1 1 0 0 1`).
2. `scale.next`: the scale a frame on is between the ends; Stop of type 1 then type 2; the
   search after; whether the stopped block keeps its scale (hypothesis `1 0 1 0 1`);
   `scale.kept` a frame later (hypothesis `1`).
3. `types`: which type numbers the translation and the rotation are (hypothesis `100 10 1
   0`).
4. `trait`: Stop and SetTrait with nothing running, SetTrait 0 and 5 on a running one
   (hypothesis `0 0 1 0`).
5. `event.set`, `event.wait`, `event.read`: the finish event's arguments, the reference,
   the block, the type, from-loop and pc (hypothesis `1120 0 0`).
6. `verts`: a node's and a missing block's vertex counts and arrays (hypothesis `0 0 0 0`;
   the original may give ar_Null, -1 for ar_Size).
7. `bars`: a global, a number, a missing bar, a UDF (hypothesis `1 -1 -1 1`).
