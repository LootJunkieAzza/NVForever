# Spec note: a crucified actor's head in conversation (issue #2471)

Tags as in `nif.md`: `[documented]` cites a page or file, `[observed]` says what was
looked at, `[hypothesis]` is a reading nothing has confirmed yet, `[design]` marks what
is NVForever's own. The freeing of Anders (#2472) is `anders-cross.md`.

## 1. What the data says

- **The cross.** FURN `NVCrucifixFurniture` 0x0012A252, model
  `meshes\furniture\nv_crucifixfurniture.nif`, marker flags 0x40000001 (bit 30, sit),
  one enabled marker: offset (0, 0, 0), orientation 0, marker ID 10; 29 placed
  references, among them Anders's 0x0013A4BA [observed: `fnvtool actor furniture`,
  2026-11-15]. The marker ID and the IDLE group the cross uses (`NVCrucifixIdles`) are
  in furniture.md §2–3.
- **The clips** the IDLE tree names for him on it: getting on
  `SpecialIdle_NVCrucifiedEnter`, the loop `DynamicIdle_NVCrucifiedIdle`, while talking
  `NVCrucifixlTalk` and listening `NVCrucifixListen` (group
  `NVCrucifixDialogueIdles`), and the fidgets `NVCrucifixScream`, `NVCrucifixGiveUp`,
  `NVCrucifixIdleFidget` [observed: the engine's log, smoke `anders-cross-head-limit`;
  furniture.md §8 for the fidgets].
- **How far the head turns.** His body part data's head-tracking part gives a Tracking
  Max Angle of 60 [documented: GECK wiki "Body Part"; observed: FalloutNV.esm BPTD
  `DefaultBodyPartData`; read as degrees to either side, hypothesis, as in
  `dialogue_look.rs`]. No field of the furniture or its marker speaks of head tracking
  [observed: the FURN record and the marker above].

## 2. What the engine showed (before the fix)

- **The body faces away from the root.** Anders's root ends with heading 90° (east),
  while his chest on the cross faces west: a player standing west of him is "-179° off
  its front" and "its chest is -0° off the player" [observed: smoke run 2026-11-15]. The
  turn limit is already measured from the chest (#1814), not the root, so the yaw the
  `stops facing` line printed (−145°, +158° in the owner's log) is the root's: about
  35° and 20° from his chest, inside the limit [observed: the same run, the player 88°
  off his chest: head at −127°, chest at −179°]. The issue's first lead (the limit not
  holding) was this reading of the log.
- **The head hangs past straight down.** In his talk and listen clips the head's
  forward points 76°–89° below the level and, at times, past straight down: then its
  forward leans a little behind the face, and the yaw read from the forward alone is the
  opposite one (+74°, −7° for a chest at −179°) with the head counted upside down
  [observed: diagnostic log, 2026-11-15]. The turn onto the player was built as "from
  that yaw and pitch, level, to the target's", so it kept the head upside down: in
  30, 30 and 20 frames at a time over two conversations, the turned head's up axis
  pointed down (up · level up = −0.9) [observed]. That is the owner's upside-down head
  in front of his chest.
- **The limping clip.** He is placed 8 units from his cross's entry point; with a
  package that holds him on it, the use began with a sidestep there at walking pace,
  on the limping clip `Left_Hurt` (his script cripples his legs on load), then the climb
  onto the cross, in sight of the player [observed: `furniture: 001413C3 goes to
  0013A4BA by marker 0 (ID 10), 8 units away` and `limping: … moves Left on its
  limping clip Left_Hurt`]. Once on the cross no movement clip plays: the furniture clip
  replaces them (`locomotion::drive`) [observed: the code].

## 3. NVForever

- A head's yaw and pitch are read with its up axis (`dialogue_look::head_yaw_pitch`):
  a head the right way up reads as before; one over the top keeps its face's yaw and
  gets a pitch past ±90°, so the turn lifts it through the level instead of turning it
  over. Head tracking reads it the same way (`head_look::ChainPose::head_angles`)
  [design]. The `stops facing` line now gives the yaw from the chest and how many frames
  the head, fully turned, was upside down. A head the right way up but rolled more
  than a quarter turn would be read as upside down too; no clip we have seen does that
  [hypothesis].
- An actor standing within a shuffle (16 units) of a cross's entry point is put on it
  at once, as one found there: no steps and no getting-on clip
  (`furniture::found_on_cross`) [design]. One walked to a cross from further still
  climbs on.
- Whether a crucified actor turns his head to the player at all, and whether the
  original shows the climb onto the cross, are not known: observer check 174, owner
  review item 83.
