# Spec notes: doors that open in place

Clean-room description in plain language. Tags: **[documented]** cites a public source,
**[observed]** was measured on the owner's install (FNV 1.4.0.525), **[hypothesis]**
needs verification. Counts come from `fnvtool sim door-models` (fnv.sim.door_models/1)
over FalloutNV.esm and the five DLC masters (DeadMoney, HonestHearts, OldWorldBlues,
LonesomeRoad, GunRunnersArsenal), models read from the default archive order.

Design: docs/design/navmesh.md §6.1 (doors and actors following paths). Code:
`fnv_collide::anim` (sequences, node poses, `DoorMotion`), `fnv_phys::PhysWorld::pose_ref_object`,
`nvf_play::doors`, nvforever `doors.rs`, `fnv_nav` (doors on paths).

## 1. What the data holds

- **DOOR record** [documented: xEdit wbDefinitionsFNV.pas DOOR, schemas/fnv/objects.json]:
  MODL model, SNAM open sound, ANAM close sound, BNAM looping sound (all SOUN), FNAM
  flags (bit 1 Automatic Door, 2 Hidden, 3 Minimal Use, 4 Sliding Door). Read by
  `fnv_world::placed::door_record`.
- **FNAM** over the 474 DOOR bases: 0x00 on 364, Sliding Door alone on 93, Minimal Use on
  13, Hidden on 2, Minimal Use + Sliding on 1, Hidden + Minimal Use + Sliding on 1
  [observed]. `UtlDoor01` (0x0003940E), a hinged-looking utility door, has Sliding Door
  set [observed]; what the flag changes in the game is not known.
- **REFR**: XTEL makes a door a teleport door (it leads elsewhere); without it the door
  opens in place. ONAM "Open by Default"; XLOC lock data [documented: xEdit REFR].
  `fnv_world::Placement::teleport` and `open_by_default` read them.
- **Models**: of the 474 bases (2 name a model missing from the archives:
  `PrisonMarker`, `CaveDoor`) [observed]:

  | Sequences in the model | Bases | Placed refs | Of them without XTEL |
  |---|---|---|---|
  | Open, Close | 263 | 2,347 | 1,844 |
  | none | 175 | 1,015 | 202 |
  | Backward, Forward, Left, Right (force-field effects: `NVMonorailDoor`, `VFSVegasGate`, …) | 29 | 64 | 2 |
  | Backward, Close, Forward, Open (`NVDLC03ForceFieldDoor*`) | 3 | 28 | 28 |
  | Backward, Forward (`ElevatorControl01`, `NVDLC01Gala02ElevatorControl`) | 2 | 3 | 3 |

  The models without sequences that stand in cells are frames and static doors
  (`OffDoorWoodFrameStatic`, `HotelDoor01`, `BlackDoor`, ladders) [observed]; they
  cannot be animated and are drawn as before.
- **Sequences** are NiControllerSequences in the model (no separate `.kf`). Each has
  NiTextKeyExtraData keys `start` and `end` and usually one or more sound keys
  [observed]. In all 658 door sequences with an `end` key it is at the stop time
  [observed]. Sound keys read `sound: <SOUN editor ID>` or `Sound: <editor ID> <node>`
  (586 keys; e.g. `sound: DRSMetalUtilitySmallOpen`, `Sound: DRSVaultGearOpenSteam01
  SoundNodeSteam`) [observed]; the trailing name looks like the node the sound comes
  from [hypothesis].
- **Lengths**: "Open" is most often 1.00 s (102 models) or 1.67 s (80), "Close" 0.97 s
  (101) or 1.67 s (74); the range is 0.03–25 s (vault doors are long) [observed].
  `UtlDoor01`: both 1.67 s, 47 tracks [observed].
- **Cycle type** clamp (2) on 265 "Open" and 257 "Close", loop (0) on 1 and 9
  [observed]. NVForever plays door sequences once and holds the end whatever the cycle
  type [engine choice].
- **Collision**: door models carry 652 keyframed rigid bodies (644 on layer 2
  AnimStatic, 8 on layer 28) and 288 static ones (frames, mostly layer 1) [observed].
  For the 610 keyframed bodies on Open/Close models, posing the model with "Open" at its
  end moves every one of them, and "Close" at its end equals the stored (bind) pose for
  606; the 4 bodies of `Lucky38ExtDoorDUPLICATE000` differ [observed]. So the stored
  pose is the closed door, and keyframed collision is meant to follow the animated nodes
  [hypothesis from the motion type, consistent with these poses].
- **Locks**: 244 placed doors without XTEL carry XLOC (241 on Open/Close models); 203
  carry ONAM [observed].

## 2. Behaviour implemented, and its status

- A door without XTEL whose model has Open and Close that move its nodes is drawn with
  its node hierarchy and posed from its state; teleport doors stay static.
- Activating it (E) toggles it when it is not locked or the actor carries the key; a
  locked door says so [observed for containers, simulation.md; doors per the GECK wiki,
  as fnv-sim already did]. Lock data means locked at the start [hypothesis, fnv-sim].
- **A door flagged Open by Default starts open** in the simulation and is drawn and
  collides open [hypothesis from the flag's name; a gate so flagged let the player drop
  through it, physics.md §4]. The shared collision rule `fnv_phys::ref_collision` still
  leaves such a door's moving parts out (no animation there); the game poses them instead
  (`RefCollisionOptions::posed_doors`).
- A door told to close while opening (or the reverse) turns back from the same fraction
  of the other sequence [engine choice; not observed].
- A sequence counts as finished at its `end` text key [engine choice from §1].
- Sounds: NVForever has no sound playback; the sound keys are logged when passed.
- `GetOpenState` still reports only 1 (open) and 3 (closed); the GECK wiki's opening
  and closing codes are not produced yet.

## 3. Open questions (not answered by the data)

1. Do doors close by themselves (behind an actor, after a time)?
2. How close must an NPC be to open a door: does it have to touch it, or does it open it
   from a distance as it approaches?
3. What "Sliding Door" changes, given that hinged-looking doors carry it.
4. Whether NPCs wait for the open animation to finish in the original game (NVForever's
   followers do, by owner decision 2026-10-09).
