# Spec notes: idle animations played on request (scripts, packages, dialogue)

Clean-room notes in plain language on the idles an actor is *told* to play: by a script's
`PlayIdle`, by an AI package (its begin, end and change idles and its idle collection), by
an idle marker, and by a dialogue response. The IDLE records, the idle tree and the `.kf`
files are in docs/spec-notes/animation.md; the editor documentation on packages, idle
collections and markers is paraphrased in docs/spec-notes/actor-behaviour.md §1.4, §1.5
and §5. The idle manager (idles chosen from the tree by their conditions with nobody
asking) is not part of this note and is not built.

Tags: **[documented]** cites a public source, **[observed]** was read from the owner's
install (FNV 1.4.0.525, `FalloutNV.esm`, `Fallout - Meshes.bsa` and `Update.bsa` only, with
`fnvtool esm decode`, `fnvtool ai show`, `fnvtool dialogue lines`, `fnvtool script
decompile`, `fnvtool nif decode` and `fnvtool nif anim --filter`), **[hypothesis]** needs
checking against the original game, **[design]** is our own choice.

Code: `crates/fnv-game/src/idles.rs` (who is asked to play what, headless),
`crates/fnv-actor/src/idle_play.rs` (one idle playing: loops, fades),
`engine/nvforever/src/idles.rs` (playing them on actors and on the player),
`engine/nvforever/src/player_body.rs` (the player's rigs and view).

## 1. Where idles are asked for

- **Scripts**: `PlayIdle <name>` takes the idle's Editor ID as text and plays it on the
  calling actor [documented: catalogue; observed: 197 calls in `FalloutNV.esm`'s scripts,
  318 in the ten masters, scripts.md §3]. The catalogue's note on `PickIdle` says the
  forced function plays an idle "regardless of conditions" [documented].
- **Packages**: each of the three state changes (begin, end, change) can name an idle
  (PACK `INAM` after `POBA` / `POEA` / `POCA`); the result script runs first, so the idle
  sees what it set [documented: GECK wiki, Category:Packages]. 51 / 25 / 23 packages name
  a begin / end / change idle [observed: packages.md §2]. A package can also carry an
  **idle collection** (`IDLF` flags, `IDLC` count, `IDLT` timer, `IDLA` idles): 267
  packages do [observed].
- **Idle markers** (IDLM) carry a collection of the same layout; a marker's idles
  override the package's [documented].
- **Dialogue**: each response can name an idle for the speaker (`SNAM`) and one for the
  listener (`LNAM`) [documented]; 1,684 responses name a speaker idle and 1 a listener
  idle [observed: dialogue.md §2].
- **Furniture**: getting on, staying on and getting off furniture are idles chosen from
  the tree by their conditions (furniture.md §3). One of them belongs here, §6.

## 2. The opening as data [observed]

The test case: Doc Mitchell's house at the start of a new game (quest VCG01).

| Moment | What the data does |
|---|---|
| VCG01 stage 0 | `player.AddScriptPackage VCG01PlayerSection0` |
| 0.2 s, then 2.8 s later (quest script timer) | stages 1 and 3; stage 3: Doc Mitchell `SayTo player VCG01Intro` |
| end of the first line | its end script sets stage 5: `player.AddScriptPackage VCG01PlayerSection1` |
| 3.25 s later | stage 7 gives the same package again, stage 8 the next line, whose response names the speaker idle `VCG01DocWhoaThere` |
| later lines | speaker idles `SitChairTalkPlayerD`, `SitChairTalkPlayerA`, `VCG01DocGiveMirror` |
| stage 30 | `DocMitchellREF.PlayIdle VCG01DocGiveMirror` |
| end of line 0x00104BF9 | `player.AddScriptPackage VCG01PlayerSection2`, and the next line |
| end of the next line (0x00104BFA) | stage 45 and `DocMitchellREF.EvaluatePackage` |
| stage 50 (3 s later) | `player.RemoveScriptPackage`; stage 55 gives the controls back |

The three player packages are Travel packages "near editor location", radius 0, Must
Complete, with these idles:

| Package | Begin idle | Collection (Run In Sequence, timer 0) | Change idle |
|---|---|---|---|
| `VCG01PlayerSection0` | `LooseVCG01PlayerWakeup` | Wakeup, Situp | none |
| `VCG01PlayerSection1` | `LooseVCG01PlayerSitup` | Bedsit | `LooseVCG01PlayerBedsit` |
| `VCG01PlayerSection2` | `LooseVCG01PlayerStandup` | Situp | none |

The idles (all childless roots without conditions, group byte 0x47: Special Idle with
the 0x40 bit):

| Idle | Loop min / max | Clip length | Text keys |
|---|---|---|---|
| `LooseVCG01PlayerWakeup` | 255 / 255 | 6.00 s | `StartLoop` 0.03 s, `EndLoop` 5.97 s, `BlendOut:10`, two `Sound:` keys |
| `LooseVCG01PlayerSitup` | 0 / 0 | 5.40 s | `BlendOut:10`, a `Sound:` key |
| `LooseVCG01PlayerBedsit` | 255 / 255 | 8.17 s | `BlenIn:30` (so spelled), `BlendOut:10` at 1.47 s, `StartLoop` 2.10 s, `EndLoop` at the end |
| `LooseVCG01PlayerStandup` | 0 / 0 | 11.13 s | `Blend:10`, two `Sound:` keys, `Enum: Left` / `Right` |

All four clips lie in `meshes\characters\_male\idleanims\` but drive nodes of the
**first-person** skeleton: `Camera1st` (priority 95), `Bip`, `Bip01 NonAccum`, the
spine, neck (`Bip01 Neck1` at 55) and head, the clavicles, upper arms and forearms (20);
15 tracks each, none for `Bip01`. `Camera1st` and `Bip` exist only in
`characters\_1stperson\skeleton.nif` (animation.md §9.5).

Doc Mitchell's packages (`VCG01DocMitchellFirstPosition`, `…BedsideStandingPackage` and
four more) each list one idle in a Random collection: `NPCSmokingIdle`, an idle of the
tree with conditions (not running, a faction, an item not equipped, a random percentage
below 30).

## 3. Playing one idle

- **Loop counts**: the editor offers "Loop Forever" (the animation loops continually)
  and a minimum and a maximum "number of times an animation will loop", all three only
  for animations that can loop [documented: GECK wiki, Idle Animations Window, §9].
  **Loop Forever is stored as minimum and maximum both 255** [observed: the owner's GECK,
  2026-10-10: `LooseVCG01PlayerWakeup` shows Loop Forever ticked with both fields at 255
  and greyed out; ticking the box sets both to 255; with the box clear any minimum and
  maximum can be entered, a maximum of 255 included]. So only 255 / 255 loops for ever
  (93 idles of the ten masters, animation.md §1.1), and any other pair is a number of
  times from the minimum to the maximum. Both 0 (1,462 idles) play the idle once
  [hypothesis: at least one pass]. The same window shows "Must return a file" ticked on
  that record, whose group byte is 0x47: bit 0x80 clear, as xEdit reads the flag
  [observed].
- **Loop stretch** [hypothesis]: a looping idle plays to its `EndLoop` key and goes back
  to its `StartLoop` key; without the keys the whole clip repeats. On its last pass it
  plays on to the end of the clip.
- **Fades** [hypothesis]: `Blend:N` and `BlendIn:N` give the fade in and `BlendOut:N` the
  fade out, N in frames at 30 a second (as for movement clips, animation.md §9.3); the
  time a key sits at means nothing (the Bedsit clip's `BlendOut` key comes before its
  loop). Without a key: 0.2 s [design].
- **End** [design]: a one-shot idle ends at the end of its clip, holds its last pose
  and fades out over its fade-out time; an idle that starts meanwhile fades in over it,
  so two idles in a row cross-fade (the sit-up into sitting on the bed).
- Not read: the `Sound:` keys and every other text key.

## 4. The player

- **Which rig** [hypothesis from §2]: an idle whose clip has a track for `Camera1st`
  plays on the first-person rig; any other on the third-person body. The game's scripts
  ask the player for both kinds side by side (`player.PlayIdle Loose1stPBrahminPush` and
  `Loose3rdPBrahminPush` in one script, neither with conditions [observed]), which fits
  each going to its own rig.
- **The view** [hypothesis]: while such an idle plays, the first-person camera takes the
  `Camera1st` node's place *and rotation*, so the animation turns the view (lying on the
  back looks at the ceiling). The camera looks along the node's +Y with +Z up, the
  actor's own axes [observed in ours: the wake-up shows the ceiling fan, the seated view
  matches the owner's capture of the game to within a few pixels at 1280 × 720 once the
  two errors below were fixed].
- **The rig's other clips** [hypothesis]: the clips share priority 20 with the standing
  idle on most bones, so both cannot play; a special idle takes the place of what the
  body does by itself. Ours keeps the body's own clips under an idle only while the first
  one fades in.
- **Looking round** [design: the owner's wish, playtest of 2026-10-11]: while an idle
  drives the first-person view, the camera node alone places the view; mouse look, the
  look test aid and the movement keys are ignored (`nvf_play::player::ViewDriven`, set by
  nvforever `player_body.rs` while `PlayerIdles::held` is set; logged as "player view:
  driven by an idle's camera node …" and "… back to the player"). The first-person rig
  keeps the heading and pitch the view had when the idle started. When the last idle is
  over, the view's heading and pitch are taken from where the animation left it. The
  original's scripts leave looking on during the wake-up (VCG01 stage 0
  `EnablePlayerControls 0 0 0 0 1`, then `DisablePlayerControls 1 1 1 1 0 0 1`
  [observed]), so the original may let the mouse turn the view there; until 2026-10-11
  ours added the view's own turning to what the node gave.
- **Where the scene plays** [design, from an observation]: the opening's start marker
  (`VCG01PlayerStartMarkerREF`, 2257.0, 2318.2, 7360.0, facing south) is in Doc
  Mitchell's bed, and our collision pushes the character about 10 units off it before
  stage 0. A view-driving idle therefore plays where the game last put the player
  (`Player::placed`: the start of a game, a door, a script's move) unless the player has
  moved by themselves since, and the character is held there, colliding with nothing,
  while such idles play. What the game does with the player's collision in this scene is
  not known.
- Errors found against the owner's capture (2026-10-10) and fixed: the hand-over of the
  view at the end of an idle fired in the one frame between the sit-up and the sitting
  loop and turned the view 11° to the right; and the character was 10 units off the
  marker.

## 5. Rules as built (`fnv_game::idles`)

- An idle asked for is queued for whoever animates actors and traced: `idle_asked`
  {`actor`, `idle`, `source`}, `source` one of `script`, `package_begin`, `package_end`,
  `package_change`, `package_idles`, `marker_idles`, `speaker`, `listener`. An IDLE that
  names no `.kf` (an inner node of the tree) is not played (`idle_not_played`)
  [hypothesis: only idles with a file can be listed in a collection, documented; that
  `PlayIdle` on an inner node plays nothing is ours].
- `PlayIdle` plays whatever its conditions say [documented]. `IsIdlePlaying` answers
  what the animating layer reports (`Game::set_idle_playing`); headless, nothing is ever
  playing (as before, when the function was a stub returning 0).
- A package's begin, end or change idle is asked for after its result script and before
  the actor's `OnPackage…` block [the first documented; the second ours]. They are not
  tested against the idle's conditions, nor are dialogue idles [hypothesis].
- **One script package** [documented: actor-behaviour.md §1.4]: `AddScriptPackage`
  replaces the actor's script package. (Before, ours stacked them, so the opening's
  `RemoveScriptPackage` brought the previous one back and the player sat up again.)
- A dialogue response's idles are asked for when the response starts: in a conversation
  the listener is the player; for `SayTo` and `Say` lines the responses after the first
  start as the time of each before passes.
- **Collections** (`IdlePicker`) [documented unless tagged]: Random picks any listed
  idle, with the idle timer between two idles; Run In Sequence goes from the top, with
  the timer between whole sequences; Do Once plays the sequence once (a random collection
  then plays one idle [hypothesis]). The first idle is not waited for [hypothesis].
- **A collection's idle must pass its own conditions** [hypothesis]: every package of
  Doc Mitchell lists `NPCSmokingIdle`, whose conditions he does not meet, and he does not
  smoke in the game [owner's capture: he sits with his hands on his knees]. Conditions
  are evaluated like any others (`Game::conditions_pass`); a function the runtime does
  not have answers 0.
- **When a collection plays** [design, simpler than the documented per-type rules of
  actor-behaviour.md §1.5]: while the actor stands where its package has it with nothing
  else to do (not walking, not on furniture, not talking) and plays no other idle; the
  idles of the marker it stands at come before its package's. The player's collection
  plays while they have a script package. A package flagged No Idle Anims plays none.
  A collection idle ends when its actor walks off.
- **A package's idles end with the package** [design]: an idle that loops for ever and
  was asked for by a package or marker leaves its loop (plays out to the end of its clip)
  when that is no longer the actor's package or marker. A one-shot idle plays to its end
  whatever happens to the package [hypothesis from §2: the stand-up is 11.1 s and
  `RemoveScriptPackage` comes about 9.4 s into it; the stand-up plainly finishes in the
  game].
- With these rules the opening needs no special case: Section 0's begin idle loops until
  Section 1's begin idle (the sit-up) replaces it; after the sit-up the collection plays
  Bedsit, which loops until Section 2's begin idle (the stand-up); the package is
  removed before the stand-up ends, so Section 2's collection (Situp) never plays.
  Why Section 0 and Section 2 list the idles they do is not explained by this reading.

## 6. Doc Mitchell getting up from his chair [observed unless tagged]

- The idle `NVchairStandDoc` names `SpecialIdle_NVDocChairFrontExit.kf` (12.7 s as
  played) and sits in the chair part of the idle tree (parent `StandUp`), with the
  conditions `GetStage VCG01 >= 10`, `GetStage VCG01 <= 50` and `GetFurnitureMarkerID ==
  14`: while the opening is between those stages, an actor getting up from a seat entered
  from the front plays it instead of the plain exit.
- Doc Mitchell leaves his chair when his package changes to
  `VCG01DocMitchellBedsideStandingPackage` (condition `GetStage VCG01 >= 40`).
- Two more records look made for the same moment and are dead data: `LooseNVDocHelpPlayerUp`
  names `SpecialIdle_NVDocHelpPlayerup.kf`, which is in neither archive, and
  `LooseNVDocChairExitLeft` names an existing file; no record of `FalloutNV.esm` refers to
  either by FormID, and neither name occurs in any script.
- Built: the furniture idles' conditions can ask the game for a quest's stage
  (`fnv_actor::furniture::UseIdles::pick_in`, `GetStage` only), so Doc Mitchell gets up by
  his own clip. Before, a condition on anything but the furniture's user failed and he got
  up by the plain exit.
- Open [hypothesis needed]: *when* he gets up. In ours his package is chosen again by the
  periodic check, which falls about 0.5 s after stage 40, some 3 s before the player's
  stand-up starts. The script calls `EvaluatePackage` on him only at the end of the line
  said during the stand-up. Whether the game holds an actor's package while it says a
  line is not known.

## 7. In the engine

- NPCs: the idle is laid over the actor's locomotion clips, or over its furniture clip
  (a seated speaker's gesture), by track priority, on two more blend layers (the idle and
  the one it replaces while it fades in); `nvf_assets::anim_play::MAX_LAYERS` is 6.
- The player: §4.
- The same slot plays the animation groups scripts ask of actors with `PlayGroup`
  (nvforever `actor_groups.rs`, activators.md §7): such a clip fades in and out as an
  idle does, keeps the actor's collection waiting while it plays, and is not reported to
  the rules as an idle (`IsIdlePlaying`); an idle asked for meanwhile replaces it.
- Log lines: `idle: <actor> plays <idle> (<length>, loop <from>-<to>, fades <in> / <out>)
  (<source>)`; for the player also the rig, the place and the facing; at the end `idle
  summary: N idles asked for (… by scripts, … by packages' begin, end and change, … from
  collections, … by dialogue lines); … started on actors, … on the player; … for actors
  not drawn, … without a usable clip`.
- Smoke test (the opening; CLAUDE.md):
  `NVF_PLAY_TEST_SCRIPT_MENUS=0.05 nvforever --data <Data> --play --new-game --no-voice
  --run-seconds 90` → exit 0, "run finished", `VCG01 stage 55 set`, and the summary "10
  idles asked for (1 by scripts, 4 by packages' begin, end and change, 1 from
  collections, 4 by dialogue lines); 5 started on actors, 5 on the player; 0 for actors
  not drawn, 0 without a usable clip"; in the "furniture:" lines Doc Mitchell (00104C0F)
  "gets off 001059B0 by clip SpecialIdle_DocSitLeftExit".

## 8. Not done, and questions for the owner

1. The idle manager: `PickIdle`, fidgets, talking gestures, hit reactions from the tree.
2. Sounds and other text keys of idle clips; the response flag "Use Emotion Animation"
   and facial emotions. (Animated objects: §10.)
3. Package begin / end / change **topics** (TNAM), and the idles of actors that are not
   residents (not handed to `fnv_ai`).
4. The per-type rules for when package idles play (actor-behaviour.md §1.5); ours is one
   rule for all types (§5).
5. Third person: a first-person idle does not move the third-person body (and the
   opening's clips have no leg tracks).
6. Captures that would settle hypotheses: the loop counts (an idle with loop 2–4 played
   by `PlayIdle`, counting repeats); whether `PlayIdle` on an idle with failing conditions
   plays; whether an actor with a collection plays an idle whose conditions fail; when
   Doc Mitchell gets up (§6).

## 9. The editor's Idle Animations window [documented]

From the GECK wiki page "Idle Animations Window" (the owner's paste, 2026-10-10), in our
words, with where ours stands:

- **Animation group section**: only one animation file plays on an animation group at a
  time, so an idle on a group ends what played on that group. Whole Body ends all other
  animations; Upper Body ends those of the upper body's groups; most idles use Special
  Idle. Ours: an NPC's idle is laid over its other clips by track priority whatever its
  group; the player's idle stops the rig's other clips once it covers the rig, also for
  the Special Idle group (§4). Neither follows the group yet.
- **Looping**: Loop Forever, minimum, maximum (§3). **Replay delay**: the time between
  replays; not used by ours.
- **No Attacking**: no attacking while the idle plays; not applied.
- **Picking** (the idle manager; it confirms the tree walk of animation.md §2.2): the top
  item matching the actor's skeleton, then its first child; an idle whose conditions fail
  is passed over for its next sibling; one whose conditions pass has its children tried
  the same way; the idle whose conditions pass and none of whose children's do is the
  one to play.
- **Must Return a File**: when the idle picked has no file, the flag decides: set, it
  is skipped as if its conditions had failed and its next sibling is tried; not set,
  nothing is played. Ours (`fnv_actor::furniture::UseIdles`, the only tree walk built)
  always goes on to the next sibling, as if the flag were set on every idle.
- **When the idle manager is asked**: `PickIdle`; getting up after a knock-down or
  unconsciousness; yielding in combat; constantly while sleeping or eating; by the
  speaker and by the listener each time a new line of a conversation is said; and at
  random times by the actor's energy level while its lower body is idle (low energy
  idles more often). None of these is built (§8 item 1).

## 10. Animated objects (ANIO): props held while an idle plays

- **The record** [documented: xEdit wbDefinitionsFNV.pas ANIO; schemas/fnv/placed.json]:
  EDID, MODL (the model) and DATA "Animation", an IDLE.
- **Counts** [observed, FalloutNV.esm with the owner's load order through NVForever's
  reader]: 161 ANIO records, 141 with a model and a DATA that names a loaded IDLE, for
  116 IDLEs; the other 20 include the 18 whose DATA lies outside the master list
  (esm.md item 7).
- **Nothing in the idle clip names the object** [observed]: `sweeping.kf`
  (`SpecialIdle_Sweeping`) drives only `Bip01` nodes and has the text keys start,
  StartLoop, five `Sound: NPCHumanSweep`, `Blend:10`, EndLoop, end. The link is the
  ANIO's DATA: `aoSweepingBroom` (0x0008945D, `AnimObjectsoBroom.NIF`) names
  `LooseSweeping` (0x0008945C, `sweeping.kf`); `aoSmokingStanding` (0x00083519) names
  `NPCSmokingIdle` (0x00071EE3) and `aoLooseSmokingStanding` (0x00071F91) names
  `LooseSmokingStanding` (0x00083518), two IDLEs that play the same
  `smokingstanding.kf`, each with an ANIO of its own, both with `aoCigarette.NIF`
  [observed]. Reading: an ANIO is drawn on the actor while the IDLE its DATA names plays
  [hypothesis].
- **Where it hangs** [observed]: the model's root (`AnimObjectRoot`, identity
  transform) carries a `Prn` string extra data naming a skeleton bone, as worn rigid
  parts do (actors.md §1.2): `aobroom.nif` `Bip01 L Hand`, `aocigarette.nif`
  `Bip01 R Hand`. `aobroom.nif` also has a transform controller on its root (not played
  here) and `BSPriority#50#` in its `UPB`.
- **In the engine** (`engine/nvforever/src/anim_objects.rs`) [design, from the above]:
  while an actor's idle slot plays an IDLE that has animated objects (until it has faded
  out), each object's model is drawn rigid under its `Prn` bone; when another idle starts
  or the idle is over, the objects are taken away. Log: `anim object: <actor> holds
  <model> on <bone> (<ANIO>, idle <IDLE>)`, and `anim objects summary: … objects drawn
  for … idles played, … could not be drawn` at the end.
- Seen in NVForever, 2026-10-11 [observed in NVForever]: Chet (GSChetRef) at his
  `GobSweep` idle marker in GSGeneralStore plays `LooseSweeping` and holds the broom in
  both hands, its head on the floor; Sunny Smiles asked to play `NPCSmokingIdle` (test
  aid `play_idle:SunnyRef:NPCSmokingIdle`) is given `aocigarette.nif` on `Bip01 R Hand`
  (log; the frame taken shows her greeting the player, a dialogue idle having replaced
  the smoking one and the cigarette gone with it).
- Not done: the player's idles, the objects' own animation, and checking against the
  game how long an object stays (the whole idle, as here, or only between keys).
