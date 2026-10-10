# Spec notes: furniture actors use (chairs, stools, benches, beds)

Clean-room description in plain language. Tags: **[documented]** cites a public source,
**[observed]** was measured on the owner's install (FNV 1.4.0.525), **[hypothesis]**
needs verification, **[design]** is our choice where the game's rule is not known.
Counts come from `fnvtool actor furniture` (fnv.actor.furniture/1, §6) over FalloutNV.esm,
models and clips read from the default archive order.

Code: `fnv_actor::furniture` (markers, marker flags, the clip an IDLE names, frames,
the phases of a use). Related: animation.md §1–2 (IDLE records and their tree),
actor-behaviour.md §1 (packages that send actors to furniture), ai-observer.md §7
(GetSitting readings in the original game).

## 1. The FURN record

- **Subrecords** [documented: xEdit wbDefinitionsFNV.pas FURN; schemas/fnv/objects.json]:
  the usual object set (EDID, OBND, FULL, MODL, SCRI, destruction data) and `MNAM`
  "Marker Flags", a byte array.
- 234 bases, 7,054 placed references; 190 bases have a display name (the 44 without
  place 695 references); 16 have a script [observed].
- **MNAM is four bytes on every base** [observed], read here as one little-endian number:
  - **Bit 30** is set on every base whose model markers have IDs 10–20 (seats, §2) and
    on 7 of the 30 bases with marker 22; **bit 31** on every base whose markers have IDs
    1–6 (beds) and on one base with marker 22; 31 bases have neither (22 with marker 22,
    2 wall-lean markers, the 7 without markers) [observed]. Reading: bit 30 "sit",
    bit 31 "sleep" [hypothesis; the names are ours].
  - **The low bits enable markers by their index in the model** [observed: `Chair01L`,
    `Chair01R` and `Chair01F` use one model whose markers are left, right and front in
    that order and carry bits 0, 1 and 2; the same for `SubChairL/R/F`; table chairs
    (`Chair01TableL/R`) have only a side enabled]. Of the 227 bases with markers, the bits
    enable every marker on 94 and only some on 133; 22 bases also set bits past the last
    marker (three-seat benches with 0x1F) [observed], which enable nothing.
  - No base with markers has all of them disabled [observed].

## 2. Markers on the model (`BSFurnitureMarker`)

- **Layout** [documented: nif.xml `BSFurnitureMarker` and `FurniturePosition` for
  Fallout 3 / New Vegas files; schemas/nif/scene.json]: an extra-data block named `FRN`
  with a count and that many positions, each an offset (three floats), an orientation
  (u16) and two position references (u8 each).
- 227 of 234 bases have a readable model with markers; one model is missing
  (`furniture\dancemarker.nif`) and six console and intercom models have none
  [observed]. Models carry 1 to 6 positions (62, 72, 73, 12, 5, 3 bases) [observed].
- **The two position references are equal on every position** [observed, 516 positions].
  The value is what `GetFurnitureMarkerID` returns [hypothesis, supported by §3: the
  IDLE tree tests exactly the values found here]. The catalogue's note says 1–10 are
  beds and 11–20 chairs [documented: GECK wiki GetFurnitureMarkerID, catalogue notes].
  Values found, with the names the IDLE tree's editor IDs suggest [observed values;
  names hypothesis]:

  | ID | Positions | Placed refs of bases having it | Reading |
  |---|---|---|---|
  | 1, 2 | 50, 46 | 615, 818 | bed, left and right side |
  | 3, 4 | 5, 4 | 502, 494 | floor bed (bedroll), left and right |
  | 5, 6 | 5, 3 | 155, 96 | sleeping on the floor, left and right |
  | 10 | 1 | 29 | (no idles test it) |
  | 11, 12 | 93, 93 | 2,547, 2,550 | chair, entered from its left and right |
  | 13 | 0 | 0 | chair from behind (idles exist, no model uses it) |
  | 14 | 153 | 3,067 | chair or bench, from the front |
  | 15 | 21 | 692 | stool |
  | 16 | 1 | 0 | pod chair |
  | 17 | 3 | 304 | sitting on the floor |
  | 18 | 1 | 10 | restrained sitting |
  | 19 | 3 | 352 | leaning on a wall |
  | 20 | 4 | 323 | child at a bar |
  | 22 | 30 | 526 | standing work spots (workbench, stove, bar, terminal…) |

- **Offset**: in the model's own space; for seats the height is the floor under the
  model (−39 to +2 across chair models) [observed], so a position is where the actor
  stands before using the furniture [hypothesis, supported by §4].
- **Orientation**: 0, 1570, 3141 and 4712 on 469 of 516 positions, the rest other values
  up to 6283 [observed]: thousandths of a radian [hypothesis]. On chairs, ID 11 has 1570
  (88 of 93), ID 12 has 4712 (88 of 93) and ID 14 has 3141 (123 of 153) [observed]. Read
  as a heading clockwise from the model's +Y, the actor at each position faces the
  furniture [hypothesis, supported by §4: the clip then carries it onto the seat].

## 3. Which clip: the furniture part of the IDLE tree

- One root IDLE, `FurnitureIdles`, has the conditions `GetSitting > 0 OR GetSleeping > 0`;
  394 IDLEs lie under it [observed]. Its two children are `Sitting` (`GetSitting > 0`)
  and `Sleeping` (no conditions).
- Below them the tree first separates furniture by `GetFurnitureMarkerID` (chairs
  `>= 11` and `<= 14`, stool `== 15`, pod 16, floor 17, restrained 18, wall 19, child bar
  20, work spots 22, beds `== 1 OR == 2`, floor beds 3/4) or by `IsCurrentFurnitureObj`
  for special furniture, then by the value of `GetSitting` / `GetSleeping` [observed]:

  | Value | What the IDLEs under it name |
  |---|---|
  | 1 | the loop played on the furniture (`DynamicIdle_ChairSit.kf`; `1stP_ChairDynamicIdle.kf` under `GetIsID Player` and `IsPC1stPerson`) |
  | 2 | the clip that gets the actor on (`Chair_LeftEnter.kf` under `GetFurnitureMarkerID == 11`, `Chair_RightEnter.kf` 12, `Chair_BackEnter.kf` 13, `Chair_ForwardEnter.kf` 14) |
  | 3 | things to do while on it: eating, talking, fidgets, each behind more conditions |
  | 4 | the clip that gets the actor off (`Chair_LeftExit.kf` …), with a "fast" set under `GetIsAlerted == 1 OR GetAttacked == 1` |

  The observer saw `GetSitting` 3 on seated actors and 4 on actors getting up
  [observed: ai-observer.md §7]. That 0 is "not using furniture" follows from the root's
  conditions. What 1 means while it lasts is not known; the tree only shows that the loop
  is looked up under it [hypothesis: the state in which the game loads the loop].
- **Order among siblings**: following the previous links (animation.md §2), special cases
  come before general ones throughout: children, the fast exits, women (`GetIsSex Female`
  → the `ChairSkirt_*` clips), a quest-specific exit, then the plain clips [observed].
  The picker therefore takes the first sibling whose conditions hold, descends, and goes
  on to the next sibling when a passing IDLE leads to no clip (the fast-exit group has no
  clip for marker 13) [hypothesis; the owner's rule for stored lists is "first eligible
  from the top"].
- The two child groups carry `GetRandomPercent < 0`, which never holds [observed].
- Conditions use these functions: GetSitting, GetSleeping, GetFurnitureMarkerID,
  IsCurrentFurnitureObj, GetIsID, IsPC1stPerson, GetIsSex, IsChild, GetIsAlerted,
  GetAttacked, GetRandomPercent, and (for what to do while seated) GetCurrentAIProcedure,
  IsTalking, GetIsUsedItem, GetIsUsedItemType, GetEquipped, MenuMode, GetStage and others
  [observed]. The picker knows the first eleven and treats an IDLE with any other
  function as not chosen [design], which leaves out only the value-3 extras.
- IDs 10 and 22 have no enter, loop or exit clip in the tree [observed]: work spots play
  their special idles while standing.

## 4. How the clips move the body

Measured on the first sequence of each clip: the translation of the `Bip01` track and the
turn about Z of the `Bip01 NonAccum` track, at the sequence's start and stop [observed].
Every one of these sequences names `Bip01` as its accumulation root (88 clips over the
marker IDs in use); the getting-on and getting-off clips move it and never turn it, the
loops have no track for it [observed]. An accumulation root's motion is the actor's own
movement (actors.md §7.2), so these clips move the actor.

| Clip | `Bip01` start → stop (x, y) | `Bip01 NonAccum` z, turn start → stop |
|---|---|---|
| `chair_leftenter` (1.13 s) | (0, 0) → (0.1, 50.7) | 66.7, 1.44 → 36.9, −3.13 |
| `chair_rightenter` (1.13 s) | (0, 0) → (0.1, 50.7) | turn 1.44 → 0.02 (height not read) |
| `chair_forwardenter` (1.73 s) | (0, −1.1) → (0, 54.3) | 67.4, 1.44 → 36.8, −1.55 |
| `dynamicidle_chairsit` (13.3 s loop) | no track | 36.8, 1.59 throughout |
| `chair_leftexit` (3.33 s) | (0.1, 50.9) → (−0.5, −0.8) | 37.5, −3.12 → 66.9, −1.70 |
| `chair_forwardexit` (1.53 s) | (0, 54.3) → (0, −1.1) | 36.8, −1.55 → 67.4, −1.70 |
| `chair_forwardexitfast` (0.47 s) | (0, 54.3) → (−2.5, 28.1) | 36.8, −1.55 → 66.5, −1.70 |
| `stoolenter` (4.63 s) | (20.5, −41.0) → (1.5, 6.2) | 62.5, 1.43 → 54.5, 1.58 |
| `dynamicidle_sitstool` (9 s loop) | no track | 54.5, 1.58 |
| `stoolexit` (4.33 s) | (1.5, 6.2) → (25.9, −40.9) | 54.5, 1.58 → 65.8, −1.72 |
| `bedleft_enter` (3.0 s) | (0, 0) → (−10.5, 68.4) | 66.7, 1.44 → 49.6, 1.27 |
| `dynamicidle_sleep` (4 s loop) | no track | 49.6, −0.30 |

What this shows:

- **The enter clip carries the body from the entry point onto the furniture.**
  `chair01.nif` has its left marker 51.9 units to the left of the model's origin facing
  it, and the left enter clip moves `Bip01` 50.7 units forward; its front marker is 54.6
  units in front, and the front enter clip moves 55.4 forward [observed]. The stool's
  marker offset is (20.5, −41.0) and `stoolenter` starts `Bip01` at exactly (20.5, −41.0):
  that clip is authored around the stool's origin, the chair clips around the entry point
  [observed].
- **The enter clip ends turned the way the loop needs.** The loop holds the standing turn
  (1.59; a standing actor has 1.44–1.59). The left enter stops 4.72 (= −1.57) from it,
  the right +1.57, the front 3.14: a quarter turn left, a quarter right, and about-face,
  each ending with the body facing the model's +Y [observed].
- **The exit clip starts where the enter clip stops** (same `Bip01` position and turn)
  and ends at the entry point turned 3.14 from standing: the actor ends with its back to
  the furniture [observed].
- Sound keys name the sounds (`Sound: NPCHumanSitChairDown`, `…SitChairUp`,
  `…SitChairUpFast`, `NPCHumanBedEnter`), and `Blend:15` / `Blend: 9` appear on most
  [observed; the blend unit is not known, animation.md §9.3].

**Model built on it** [hypothesis for the game, design here; `UseFrames`]:

1. The actor stands at the entry point, facing along the marker's heading (§2).
2. Getting on plays in a frame placed so that the clip's first `Bip01` position is the
   entry point (for chair clips, the entry point itself); the actor's root follows the
   `Bip01` track from there, keeping the entry heading.
3. When it ends the root is where the clip left `Bip01`, and it turns by the difference
   between the clip's last body turn and the loop's; the loop plays there.
4. Getting off plays in the frame of step 2 again, the root following the clip's `Bip01`
   track; when it ends the root turns by the difference from the standing turn.

Checked on `chair01` from the left and the front, unturned and turned: the seat frame
lands within 2.5 units of the model's origin facing its +Y, and the actor ends within 1.5
units of the entry point with its back to the chair (`fnv_actor::furniture` tests). Seen
in the engine: §7.

## 5. Open questions

1. **Which marker an actor takes** when several are enabled, and whether it must be
   reachable on the navmesh (level-design.md §6 says the approach side needs navmesh).
   Here: the nearest enabled one [design].
2. **Whether the player walks to the marker** or is put there. Here: put there [design].
3. **Occupied furniture**: one user per marker, per seat or per reference? Benches have
   one marker per seat; chairs three markers for one seat. Here: one user per reference
   unless every marker has ID 14 or 15 (benches, tables of stools), where it is one per
   marker [design].
4. **GetSitting 1**, and whether the loop is chosen once or looked up again.
5. **The blend keys** and how the game crosses from clip to loop.
6. **Camera**: what the first-person view does while getting on. Here the view is the
   third-person body's eye, the body not drawn [design]. On a bed the view lies down
   with the body (§7) [engineering choice].
7. **MNAM bits 30 and 31** against the editor's own labels.
8. **Marker ID 10** (one model) and the six furniture bases without markers.
9. **How sitting ends for the player** and what the prompt says (the crosshair shows
   "Activate" here). Here: a movement control, activating the furniture again, or
   Activate with nothing under the crosshair [design].
10. **Beds and the player.** In the original the player sleeps through a menu and does
    not lie down [hypothesis from how the game is played]. Here the player lies down on
    a bed as a sleeping actor does [owner request 2026-10-10; not the original's]: the
    same markers, clips and frames as an actor on that bed, by the rules of a seat (§7).
    The sleep menu is not built and no game time passes.

A capture can answer 1, 3 and 4: an NPC sent to `Chair01F`, `Chair01L` and a bench from
different sides with the observer logging GetSitting and GetFurnitureMarkerID per frame.

## 6. CLI contract

`fnvtool actor furniture --schema <schemas/fnv> [--nif-schema <dir>] [--summary-only]
<Data> <plugins…>` prints `fnv.actor.furniture/1`:

- `summary`: counts of bases, placed references, models without markers, bases per marker
  count, positions per reference pair, per orientation, per ID and height, bases per
  marker-flag value, how the low flag bits compare with the markers, positions per type
  bits and ID, placed references per marker ID, the size of the furniture IDLE subtree.
- `clips`: per marker ID in use, the clip the IDLE tree names for a grown man who is not
  the player (`loop`, `get_on`, `get_off`, `get_off_alerted`), for a woman (`get_on_female`)
  and for the player in first person (`loop_first_person`): path, IDLE editor ID, times,
  cycle type, text keys and `body` (the `Bip01` translation and `Bip01 NonAccum` turn at
  start and stop); null where the tree names none.
- `furniture` (unless `--summary-only`): per base its FormID, editor ID, name, model,
  marker flags, sit and sleep bits, enabled marker indices, script flag, placed count,
  marker IDs and markers.

## 7. In the engine

Code: `engine/nvforever/src/furniture.rs` (actors and the player), `nvf_play::furniture`
(the activation message, the hold on the player, telling the script runtime),
`locomotion::drive` (an actor's furniture clip replaces its other clips),
`player_body.rs` (`pose_furniture`).

- **Actors.** A `SitOn { furniture, stay }` component on an actor's root starts a use:
  the nearest enabled, free marker is the entry point; the actor is moved the last steps
  to it at walking speed (no path; refused beyond 400 units) [design]; then the clips and
  frames of §3–4. `GetUp`, or `stay` seconds on the furniture, starts getting off. When
  it is off, the components are removed and a traveller's position and heading are
  updated. One user per reference, except where every marker has ID 14 or 15 (one per
  marker) [design, §5 item 3].
- **`--npc-sit ACTOR=FURNITURE[:SECONDS]`** sends a travelling NPC (docs/design/navmesh.md
  §5) to a furniture reference (hex FormID) and gives it a `SitOn` on arrival.
- **The player** uses furniture by activating it. The third-person body plays the clips
  at the frames of §4; movement, jumping and the character's collision are off
  (`PlayerSeat`), the weapon is put away [design], and a movement control or activating
  the furniture again gets up [design, §5 item 9]. Afterwards the character stands at the
  entry point. In first person the view is the body's eye and the body is not drawn
  (with `--first-person-body` it is, and the view is held to a neck's reach:
  docs/design/player-body.md §1.5).
- **The player on a bed** [owner request 2026-10-10; engineering choices, the original's
  player never lies down]. Activating a bed is a use like any other: the entry point is
  the nearest enabled marker (IDs 1–6), the clips are the ones the IDLE tree names under
  `GetSleeping` (on `NVbedtwin01` from marker ID 1: `SpecialIdle_BedLeft_Enter`, 3.0 s,
  then the loop `DynamicIdle_Sleep`, then a getting-off clip of 3.1 s [observed in
  NVForever]), and the rules are a seat's: movement, jumping and collision off, weapon
  away, up again on a movement control or Activate, standing at the entry point
  afterwards. Scripts see `GetSleeping` 2, 3 and 4 for the player and `GetSitting` 0.
  - **The loop lies on its side** [observed in NVForever on that bed: the pelvis faces
    along the mattress, one side down, knees drawn up, a hand by the face].
  - **First-person view** (`player_body::lying_view`), the same with the body drawn or
    not: the view keeps its place in the body's own frame (its pelvis's turn since the
    bind pose) while the body lies down and gets up, so the picture leans over with the
    body and, lying, shows the room as a head on a pillow sees it, on its side [owner
    request 2026-10-10: "sideways the camera when laying"]. The mouse turns and tilts the
    head in that frame, at most 80° to either side and 75° towards the feet (the seated
    limits of player-body.md §1.5), and, the further the body has lain back, the less
    below level, down to 10°: lower is the pillow and the mattress. With
    `--first-person-body` the eye also goes round the neck with the view, about the
    body's length. The player's heading and pitch are the level view nearest to the
    leaning one (what the crosshair and scripts use).
  - **Third-person view**: the camera's pivot is lifted up to 40 units above the head as
    the body lies back; left by the head, the camera's way round the body ran into the
    bed and the picture was the inside of the mattress [observed in NVForever].
  - The crosshair lying down is rarely on the bed, so Activate with nothing under it
    gets up (§5 item 9).
- **Scripts.** While a use lasts, `GetSitting` / `GetSleeping` give 2, 3 and 4 for the
  phases and `GetFurnitureMarkerID`, `IsCurrentFurnitureRef` and `IsCurrentFurnitureObj`
  answer for the user. State 1 is never reported (§5 item 4).
- **Clips change without a cross-fade**: the clips of one use play in different frames
  (§4), so blending two of them would swing the body [design].
- **Not done**: work spots (marker 22: no getting-on clip), fidgets, eating and talking
  idles while seated (value 3 of §3), the fast exits (nothing sets alerted or attacked),
  children, checking the entry point against the navmesh, the sleep menu and passing
  game time on a bed (the player only lies down), a bed's owner or a sleeper already on
  it (the bed is then "in use"), and actors choosing furniture by themselves (AI
  packages).

Seen on the owner's install, 2026-10-10 [observed in NVForever, not in the original
game]:

- `--cell Goodsprings --play --npc-sit EasyPeteRef=0010634A:14`: Easy Pete walks to his
  chair on the saloon porch (`HotelChair01F`, front marker, ID 14), turns and sits facing
  away from the wall, and after 14 s stands up in front of it with his back to it. The
  player, started next to the neighbouring chair (0x00106349), sits on activating it and
  stands on pressing Forward.
- `--cell GSProspectorSaloonInterior --play --npc-sit SunnyRef=00106194:30 --npc-sit
  00104F08=00107C9E:30`: Sunny Smiles sits on a bar stool (ID 15, `SpecialIdle_StoolEnter`),
  a settler slides into a booth from its left (`DinerBooth02TableL`, ID 11,
  `SpecialIdle_Chair_LeftEnter`), and the player sits on the next stool.
- `--cell GSHouseInterior03 --play` started next to Easy Pete's bed (0x00176ACE,
  `NVbedtwin01`), unattended, in third person and in first person with and without
  `--first-person-body`: the player sits on the edge, lies down on its side, and gets up
  to stand beside the bed on Activate; lying, the first-person picture is on its side
  with the pillow along one edge, shows the ceiling, the walls and the foot of the bed
  when turned there, the body's own shoulder and arm at the limit towards the feet, and
  never the inside of the body or the mattress. Before the view was adapted the picture
  lying down was the mattress or the inside of the body, in both modes.
