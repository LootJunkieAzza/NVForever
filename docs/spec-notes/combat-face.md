# Spec note: the combat face (CombatAnger)

Owner request 2026-10-18: actors look angry while they fight, as in the base game. This
note says what is known about the face an actor shows in combat; the morphs themselves
and how they are drawn are in lipsync.md §4.

## 1. What is documented

- The console command `MFG` sets three kinds of face movement: **expressions**
  ("full-face emotions"), phonemes (the mouth) and modifiers (eyes and brows). "An actor
  may only have one Expression at a time; calling an Expression change on a Sad actor
  will overwrite their sad demeanour." Expression 14 is **CombatAnger**; the list is
  0 Anger, 1 Fear, 2 Happy, 3 Sad, 4 Surprise, 5 MoodNeutral, 6 MoodAfraid, 7 MoodAnnoyed,
  8 MoodCocky, 9 MoodDrugged, 10 MoodPleasant, 11 MoodAngry, 12 MoodSad, 13 Pained,
  14 CombatAnger, each 0–100 [documented: GECK wiki, "ModifyFaceGen", local export
  `C:\fnv-private\geckwiki\functions.xml`].
- "MFG face emotions take a second or two to initiate on the target actor. Unfortunately,
  they are not instantaneous." [documented: same page, Notes]. So the game eases a face
  into an expression rather than switching it.
- Face movements can be keyed in `.kf` animations: a controlled block on node
  `HeadAnims:0`, controller `NiGeomMorpherController`, with the morph's name as its
  interpolator ID [documented: same page, Notes].
- A dialogue response carries an emotion: `GetDialogueEmotion` returns 0 Neutral,
  1 Anger, 2 Disgust, 3 Fear, 4 Sad, 5 Happy, 6 Surprise, 7 Pained [documented: GECK
  wiki, "GetDialogueEmotion"]; `GetDialogueEmotionValue` "does not appear to work, and
  always returns 0" [documented: same]. `ToggleEmotions` (TEMO) "disables/enables facial
  emotions of all actors" [documented: GECK wiki, "ToggleEmotions"].

Nothing found in the wiki export says when the game itself shows CombatAnger.

## 2. What the data shows [observed]

- **Which parts carry it** (`fnvtool actor tri`, the 49 `.tri` files of the base
  archives): every head (`headhuman`, `headfemale`, `headold`, `headoldfemale`,
  `headchild`, `headchildfemale`, `headghoul`, `headghoulfemale`), the three lower
  teeth, all 17 beards, the three eyebrows and the eye patch have a `CombatAnger`
  difference morph. The mouths, upper teeth, tongues and eyes do not. On `headhuman`
  it moves 822 of 1,211 vertices, at most 0.63 units at full weight (the head is about
  20 units tall); for comparison MoodAngry moves 152 vertices, at most 0.13.
- The mood labels are spelled `MoodNeutral`, `MoodAfraid`, `MoodAnnoyed`, `MoodDrugged`,
  `MoodPleasant`, `MoodAngry`, `MoodSad`, `Pained`, `CombatAnger`; MoodCocky is `Cocky`
  on four heads (`headhuman`, `headold`, `headchild`, `headghoul`) and `MoodCocky`
  elsewhere. Not every beard has every mood (`beardfull` has no MoodAngry).
- **No animation carries it.** Of the 2,450 `.kf`/`.nif` files under
  `meshes\characters` in `Fallout - Meshes.bsa`, 275 have `NiGeomMorpherController`
  blocks on `HeadAnims:0`. Their morph names are phonemes, blinks, brows, squints,
  looks, the six expressions, Pained and `Base` (281 blocks, the most common; the
  plain idle `mtidle.kf` has only that one). **None names CombatAnger or any Mood**.
  The attack, aim, reload and equip clips (`1hpaim.kf`, `1hpattackright*.kf`,
  `1hpreloadb.kf` …) have only `Base` (and the slingshot's `##dgRubberBand_target`);
  the files with expression tracks are idles and special idles (dancing, drinking at a
  table, `laughandpoint.kf`).

So the combat face does not come from the combat animations; the game must set it from
the actor's state [hypothesis: a code path like MFG Expression 14, which the expression
list's own "CombatAnger" entry beside the moods suggests].

## 3. What is not known [hypothesis until observed in the game]

| Question | NVForever's choice | Why |
|---|---|---|
| When the face is shown | while the actor's alert state is **Combat** (docs/design/alert.md), not in Alert or Lost (searching) | the morph's name; an actor searching has not started fighting |
| Strength | weight 1.0 (the whole morph) | MFG 100 is the top of the scale; nothing says the game uses less |
| Fade in / out | linear, 1.0 s in on entering combat, 1.0 s out on leaving | "a second or two to initiate" (§1); the way out is not described |
| Who shows it | every drawn actor with a face rig (heads with `.tri` files: humans and ghouls, children too); creatures and robots have no `.tri` (lipsync.md §4.3) so show nothing | the morph is in every head and ghoul head |
| The dead | the face eases out at death like leaving combat | not known what a corpse's face keeps |
| The player | not shown (the player has no alert state; the third-person player face is not driven) | not known |
| Speaking while fighting | lip sync goes on over the angry face: the mouth shapes add to it (lipsync.md §4.3) | a fighter's barks lip-sync in the game [owner, 2026-10-18 request] |
| A response with an emotion | an expression is exclusive (§1): while a response with a non-neutral emotion is said, its emotion takes the expression's place and CombatAnger eases out, coming back after; a Neutral response keeps it | "one Expression at a time" (§1); how the game ranks a dialogue emotion against combat anger is not known |

What a capture would settle: an NPC turned hostile in front of the camera (a Fiend at
one of the alert-states test spots), filmed at the dialogue camera's distance
(actors.md §4.3) from before the fight starts until a while after it ends, would show
whether the face changes at the moment of Combat or Alert, how long the change takes,
and whether it goes back.

## 4. What the runtime does

See `engine/nvforever/src/combat_face.rs` and its log line `face: <ref> mood CombatAnger
<weight>` (logged on entering and leaving combat and when the fade completes).
