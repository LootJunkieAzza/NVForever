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
| 10–11 | dodge, wait for lower body ("may be left overs") | never: no observer sample read them [observed] |
| 12 | wait for special idle | the player's landing clip after a jump or fall [hypothesis; observed: the player read 12 after a jump, below]; an NPC's or creature's landing clip the same (actor-jumps.md §5) [hypothesis: no NPC was seen landing] |
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
- 10, 11 and 12 never appeared on the actors of the fights.
- **The player** [observed: capture `player_actions_anim`, 2026-10-18, save nvfstrip, every
  game loop at about 60 a second]: −1 at rest and with a weapon in hand; drawing read 0
  for 46 (9mm pistol), 47 (.357), 61 (Cowboy Repeater), 86 (frag grenade) and 79 (knife)
  samples, holstering 1 for 79; after a jump, −1 for about 50 samples (in the air), then
  **12** for about 49. NVForever answers 12 while the player's landing clip plays.

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

### 1.1 The anim_actions campaign (2026-10-18)

Observer campaign `anim_actions` (save nvfstrip, plugin v7; NCR troopers spawned and set on
the player in god mode, one weapon each, every game loop; private outputs
`C:\fnv-private\capture\out\obs_anim_actions_*.jsonl`) [observed]:

- **Throws** (dynamite, four throws): 5 for 52–58 samples, then **6 for 11–40 (median
  36)**, then 4 (16–23) or −1. NVForever holds 6 for 0.6 s (`THROWN_SECONDS`). The frag
  grenade troopers did not throw; one throwing-spear trooper began a 5 the session ended
  on.
- **17**: on the .357 (three reloads: 17 for 113–115 samples, then 9 for 119–133) and the
  hunting revolver (two: 17 for 75–81, 9 for 105–106), always between the last shot's 4
  and the reload's 9. The 9mm pistol's three reloads went from 4 to 9 with no 17. The
  Cowboy Repeater and hunting shotgun troopers did not reload in their sessions.
- **10, 11 and 12** never appeared on these NPCs.
- **3** after a shot: one sample on the .357 and the 9mm (46 shots); 23–24 on the single
  shotgun, 27–28 on the varmint rifle, 30–31 on the hunting revolver, 30–37 on a hunting
  rifle: the guns that cycle by hand hold 3 through the cycle. NVForever still holds 3
  one frame for every gun [gap left].
- **0** drawing 46–86 samples, **1** holstering 35–75, **13** 78–93 (staggers, getting up).

## 4. IsPlayerActionActive

Its argument is an ActionIndex, 1 to 10; the list is a wiki template the export lacks
[documented: page IsPlayerActionActive, "{{:ActionIndex}}"; GetLastPlayerAction includes
the same missing template]. No other source in the private references lists it
(searched 2026-10-18: the GECK wiki export, the Fallout wiki pages). The captures above answer 2 (a throw) and confirm 9; the rest read 0. Known from the page's text:
9 is PLAYER_ACTION_IRON_SIGHTS and does not return true while the player is in combat; 8
returns 1 while the player stands on a table or chair (the page's example); firing a weapon
is an action that stays active about two game minutes, its index not given. Both shipped
calls (Old World Blues) ask 9 [observed: decompiled scripts,
docs/design/script-function-audit.md]. NVForever answers 9 (aiming down a gun's sights, not
while the player is hunted) and reads 0 for the others [not answered].

What the same capture read [observed]: IsPlayerActionActive 1–10 all 0 at rest, while
drawing and holstering, after the sneak key and after a jump; GetLastPlayerAction 0
throughout. The job's aiming, firing, throwing, swinging and blocking never happened:
`HoldControl` and `ReleaseControl` did not compile in the runtime compiler (`TapControl`
did). The follow-up `player_actions_anim2` taps instead.

`player_actions_anim2` (2026-10-18, the inputs by TapControl, aim by HoldKey 257, values
moving throughout) [observed]:

- Pistol shot: 2 for 3 loops, 3 for 1, 4 for 28–32; the same while sneaking. .357: 2 for
  2–3, 3 for 1, 4 for 44–46. Cowboy Repeater (lever): 2 for 3, **3 for 20**, 4 for 26
  (3 lasts through the lever, as on the NPCs' hand-cycled guns, §1.1).
- Grenade: 5 for 50–52, 6 for 39–41, 4 for 16–17. These are the throw clip's own keys:
  `1gtattackthrow.kf` has `Hold` 0.567 s, `Release` 0.867 s, `Attach` 1.533 s and ends at
  1.7 s [observed: `fnvtool nif decode`]. NVForever's throws now leave the hand at
  `Release` (they left at the press: the clips have no `Hit` or `Fire` key, so no 5 was
  ever told, smoke `anim-state-throw` 2026-10-18) and read 6 until `Attach`
  (`throw_action`; `THROWN_SECONDS` only for a clip without one).
- Knife: 2 for 17, then 4 for 49–50 (no 3), as `swing_action`.
- Jump with a pistol drawn: −1 for 50, 12 for 54. Holster: 1 for 78.
- The reload taps did not reload (the .357 emptied by six shots did not reload itself
  either), so the player's 17 is not settled.
- **IsPlayerActionActive**: **9** while aiming (HoldKey 257, out of combat); **2** from a
  grenade throw's first loop of 5 for 181 loops (about 2.6 s), NVForever's
  `THROW_ACTION_SECONDS`; nothing for firing (pistol, revolver, lever-action), sneaking,
  firing while sneaking, jumping, swinging, drawing or holstering. GetLastPlayerAction
  read 0 throughout, even after the throws.

`functions_round` session `mines` (2026-10-19, AI observer, plugin version 9; three frag
mines equipped, Attack tapped twice) [observed]:

- The first tap drew the mine (GetAnimAction 0 for 85 loops); the second threw it: 5 for
  48 loops, 6 for 30, 4 for 48, then −1.
- **IsPlayerActionActive 4** read 1 from the first loop of the 5 for 181 loops, 3.02 s by
  the game clock (the grenade's index 2 lasted 181 loops too, above), and no other index
  moved. **GetLastPlayerAction** read 4 over exactly the same loops and 0 before and
  after (after the grenade throws of `player_actions_anim2` it read 0 throughout; no
  shipped script or condition asks it, so NVForever leaves it at 0).
- NVForever: a throw with a mine weapon (WEAP animation type "Land Mine" or "Mine
  Drop") answers 4 instead of 2 (`AnimState::mine`, `Combat::player_weapon_is_mine`);
  `THROW_ACTION_SECONDS` is 3.0 s (was 2.6, from the loop count at another job's frame
  rate). Still unanswered: 1, 3, 5, 6, 7, 8 (8 is standing on a table by the wiki's
  example), 10; firing, jumping, swinging, sneaking and drawing read nothing.

## 5. Open, and the captures that settle it

- What 17 is exactly (§1.1 settles where it falls, not what it covers); 3 through a hand
  cycle (§1.1: it does, not reproduced yet). AI observer campaign `tools/capture/observe/anim_actions.campaign.json`
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
