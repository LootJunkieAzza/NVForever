# Animation state the scripts read

What the original's scripts and conditions can ask about an actor's body, and the
evidence for each answer. Implementation: `crates/fnv-game/src/anim_state.rs` (the rules'
snapshot and the code tables) and `engine/nvforever/src/anim_state.rs` (the engine tells it
every frame). Sources: the GECK wiki export (owner's, C:\fnv-private\geckwiki, pages
GetAnimAction, IsMoving, IsAnimPlaying, IsIdlePlaying, IsPlayerActionActive) and the AI
observer's campaigns (nvf_capture `observe` jobs, C:\fnv-private\capture\out and
\observe, recorded 2026-10-16 and 17; private, not committed).

## 1. GetAnimAction

The wiki gives two tables; the New Vegas one is used [documented: GECK wiki GetAnimAction,
"revised for GECK NV", "the NV ones are correct as to the latest info garnered from
extensive testing"]:

| Code | Meaning (wiki) | NVForever |
|---|---|---|
| −1 | none | nothing below |
| 0 | equip weapon | drawing |
| 1 | unequip weapon | putting away |
| 2 | attack: "firing full auto or doing a melee swing" | a shot's start, full auto, a blow before its strike |
| 3 | follow-through: "returned when firing semi auto" | the frame after a shot |
| 4 | "returning to the static animation after the attack", also "drawing another throw-able weapon after one was just thrown" | the rest of a shot's, blow's or throw's clip |
| 5 | "gearing up to throw" | a thrown weapon (grenade, spear, mine: one that spends itself) until it leaves the hand; the player only, NPCs throw nothing yet |
| 6 | "the throwable weapon has left the player's hand" | the frame after it left, as 3 after a semi-automatic shot [hypothesis] |
| 7 | block | blocking, a block's reaction |
| 8 | stagger: "staggered by an npc that is blocking" | a blocked blow's recoil |
| 9 | reload | any reload, round by round included |
| 10–12 | dodge, wait for lower body, wait for special idle ("may be left overs") | never: none of the observer's samples read them [observed] |
| 13 | "every action not included above": furniture, PlayIdle idles, crippled-limb staggers, getting up | PlayIdle and package begin/end/change idles, cowering, PlayGroup groups, crippled-limb staggers, a jam (player) |
| 17 | not in the table | a round-by-round reload (.357 revolver, lever-actions, hunting shotgun) while its opening clip plays, player and NPCs [hypothesis, below] |

What the observer recorded at about 60 samples a second [observed: `anim_action` field,
`obs_goodsprings_fight_fast_01_gunfight` and the 15 `obs_melee_spacing_*` runs; weapons
named with `fnvtool combat form`]:

- **Semi-automatic guns** (.357 Magnum Revolver, Single Shotgun): each shot 2 for 2–4
  samples, 3 for one sample, 4 for 13–35 samples, then −1 or the next shot.
- **Bolt-action rifles** (Varmint Rifle, Hunting Rifle): 2 for 2–4 samples, 3 for 21–37
  samples (the bolt), 4 for about 20. NVForever holds 3 one frame for every gun
  [hypothesis left: the bolt's length is not told apart].
- **Reloads**: 9 for about 90–250 samples after a magazine's last shot, every gun (once 26, cut short by a stagger's 13).
- **17**, a code the wiki does not list: on .357 revolvers only, 89–116 samples, directly
  before many (not all) of their reloads' 9. The .357 is the only round-by-round loader in
  those runs, so NVForever answers 17 while such a reload opens the weapon
  (`ReloadOpen`) and 9 for its rounds and close [hypothesis]. Against it: the opening clip
  `1hpreloadxstart.kf` lasts 1.07 s (64 samples) while 17 lasted 1.5–1.9 s; the round
  clip `1hpreloadx.kf` lasts 1.77 s [observed: `fnvtool nif anim`, Fallout - Meshes.bsa],
  so 17 may be a round's phase instead. The campaign `anim_actions` (§5) settles it.
- **Melee and creatures**: the baseball bat 2 for 42 samples, then 4 for 60; creatures
  (dogs, geckos, radscorpions …) 2 for 58–75, then 4 for about 20; no 3. NVForever
  switches from 2 to 4 at the clip's `Hit` key [hypothesis].
- **Drawing and holstering**: 0 for 36–159 samples and 1 for 30–85, as before
  (`fa_batch3_actions`: Ringo 0 three frames after SetAlert 1).
- **13**: 1,163 samples at AI procedure 7, 835 at 44, 649 at 0 and some in combat, against
  hundreds of thousands of −1 for the same procedures: sitting, talking and standing
  actors read −1 nearly always, so collections' idles, fidgets and gestures are not 13.
- 5 (79 samples) and 6 (10) appeared in combat only, on throwers. NVForever reads a
  throw as a shot: 5 until the weapon leaves the hand (the clip's fire time), 6 the frame
  after, then 4 (`anim_state::throw_action`); the length of 6 is a hypothesis.
- 10, 11 and 12 never appeared in any sample.

## 2. IsMoving

0 not moving, 1 forward, 2 backward, 3 left, 4 right, the way the actor "is currently
requesting to move" [documented: GECK wiki IsMoving]. The player's way is the movement keys
(held even against a wall); an NPC's the way its body walks, in its own axes; a diagonal
reads the larger of the two axes, forward and back on a tie [hypothesis]. 0 at rest
[observed: function probes 2026-10-17]. The observer's campaigns did not record IsMoving.

## 3. IsAnimPlaying and IsIdlePlaying

- Sunny Smiles standing read `IsAnimPlaying Idle` 1 [observed: function probes
  2026-10-17]: an actor's own movement group counts. NVForever tells the group of the
  actor's locomotion clip (`Idle`, `Forward`, `FastForward`, `Left` …, the clips' names
  [observed: `mtidle.kf`, `mtfastforward.kf` in the base archives]) and that walking and
  running groups count the same way [hypothesis]. An actor not walking reads `Idle`
  while it turns in place too (its clip is then TurnLeft or TurnRight) [hypothesis;
  Chet standing at his counter read 0 before this, smoke `anim-state` 2026-10-18]. A script's PlayGroup group comes first.
- Easy Pete sitting read IsIdlePlaying 1 with GetSitting 3 [observed: function probes
  2026-10-17]: an actor on furniture plays an idle; NVForever reads 1 at GetSitting or
  GetSleeping 3 [hypothesis for beds].

## 4. IsPlayerActionActive

Its argument is an ActionIndex, 1 to 10; the list is a wiki template the export lacks
[documented: page IsPlayerActionActive, "{{:ActionIndex}}"; GetLastPlayerAction includes
the same missing template]. No other source in the private references lists it
(searched 2026-10-18: the GECK wiki export, the Fallout wiki pages). Known from the page's text:
9 is PLAYER_ACTION_IRON_SIGHTS and does not return true while the player is in combat; 8
returns 1 while the player stands on a table or chair (the page's example); firing a weapon
is an action that stays active about two game minutes, its index not given. Both shipped
calls (Old World Blues) ask 9 [observed: decompiled scripts,
docs/design/script-function-audit.md]. NVForever answers 9 (aiming down a gun's sights, not
while the player is hunted) and reads 0 for the others [not answered].

## 5. Open, and the captures that settle it

- What 17 means; whether 3 lasts through a bolt or pump cycle by the clip's keys; how
  long 6 lasts. AI observer campaign `tools/capture/observe/anim_actions.campaign.json`
  (nvfstrip): NCR troopers each armed with one weapon and its ammo (the place item's
  `give`, new) set on the player in god mode: frag grenades, throwing spears, dynamite,
  the .357, the Cowboy Repeater and the hunting shotgun (round by round), the hunting
  revolver, the 9mm pistol and the single shotgun (controls without 17), the varmint
  rifle (the bolt); `anim_action` every game loop.
- The ActionIndex list, and the player's own codes: capture job
  `tools/capture/jobs/player_actions_anim.steps.json` (nvfstrip) samples, every game loop,
  (GetAnimAction + 1) × 2048 plus the bits of IsPlayerActionActive 1–10 while the player
  (god mode, an empty house) draws, aims, fires, reloads the 9mm, the .357 (emptied, and
  part-empty) and the Cowboy Repeater, sneaks, jumps, throws two grenades, swings and
  blocks with a knife, and reads GetLastPlayerAction after each.
- Whether IsAnimPlaying's walking and running groups answer on actors as Idle does.
