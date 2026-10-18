# Spec notes: unconscious actors (essential knockouts and fatigue knockouts)

Actors that fall unconscious instead of dying, and get up again: essential actors at zero
health, and any actor whose Fatigue runs out (boxing gloves). Owner request 2026-10-17.
Code: `crates/fnv-game/src/unconscious.rs` (the rules), `crates/fnv-game/src/combat.rs`
(where hits lead to them), `engine/nvf-play/src/knockouts.rs` (corner message, log lines,
the `NVF_PLAY_TEST_HIT` aid), `engine/nvforever/src/knockouts.rs` (falling and getting up
on screen).

## 1. What the data and the documentation say

- **Essential** is an NPC_/CREA ACBS flag [documented: xEdit flag name]; a script sets
  it on a base (SetEssential) or a reference (SetActorRefEssential) [documented:
  catalogue]. KillActor on an essential actor makes it "temporarily unconscious for a
  few seconds" and KillAllActors does the same, "in their normal temporary dead state"
  [documented: GECK wiki KillActor and KillAllActors notes, owner export
  `C:\fnv-private\geckwiki\functions.xml`].
- **Fatigue** is actor value 22 [documented: actor-values.md]. A creature's ACBS fatigue
  is its fatigue, "unconscious at zero in New Vegas" [documented: GECK wiki Stats Tab -
  Creatures, combat.md §4.2]; NPCs carry the same ACBS field (Goodsprings residents: 50
  [observed 2026-10-17, `fnvtool esm decode`: GSSunnySmiles, GSRingo, GSTrudy,
  GSEasyPete, DocMitchell]) and NVForever reads it as theirs [hypothesis, as
  actor-values.md].
- **Boxing gloves** deal fatigue through their weapon script, not through the EITM
  effect [observed 2026-10-17]: WEAP `WeapNVBoxingGloves` (0x000E5391) has SCRI
  `BoxingGlovesFatigueOnHitScript` (0x001489D0) and EITM `DummyNVBoxingGloves`, whose
  only effect `DummyNVFatigueDamage` ("Inflicts Fatigue Damage (KO)") is a Value
  Modifier on no actor value. The script's source (SCTX) is one `Begin OnHit` block: it
  takes `GetOwnerLastTarget` and, unless that is the player or a creature of type 6,
  does `DamageAV Fatigue 35` on it, else `DamageAV Health 10`. The Golden Gloves, the
  Boxing Tape and the cattle prod have scripts of the same kind
  (`BoxingGoldenGlovesFatigueOnHitScript`, `BoxingTapeFatigueOnHitScript`,
  `CattleProdFatigueOnHitScript`).
- **GetOwnerLastTarget** "gives the most recent target of whoever owns the calling
  reference" [documented: catalogue summary].
- **GetKnockedState** returns −1 for a fatigue knockout, 0 normal, 1 knocked out
  ("includes unconscious essential actor, paralyzed actor, and negative fatigue
  knockout"), 2 knocked down [documented: GECK wiki GetKnockedState].
- **GetUnconscious** reads only SetUnconscious's state; it "does not detect actors who
  have been rendered unconscious from fatigue damage or from being essential", for which
  GetKnockedState is meant [documented: GECK wiki GetUnconscious]. **GetDead** does not
  read 1 for an essential actor knocked out [documented: GECK wiki GetDead notes].
- **IsFacingUp** (bipeds) and **IsLeftUp** (quadrupeds) are the idle manager's checks of
  a knocked-down actor's side [documented: GECK wiki].
- **KOed** combat topic: "said when an actor loses all their fatigue points and becomes
  unconscious" [documented: GECK wiki Combat Tab].
- **Combat style**: "Unconscious Bonus to Attack / to Power Attack" are added when the
  opponent is unconscious [documented: GECK wiki Combat Style] — in the original an
  unconscious opponent can still be attacked. The owner asked that an unconscious actor
  no longer be targeted (§3) [design, owner request 2026-10-17].
- **Text**: GMST `sNoTalkUnConscious` (0x000A9D13) is "is unconscious." [observed
  2026-10-17, its DATA bytes]. No GMST names an essential's down time, the health it gets
  up with, or how fast fatigue returns: FalloutNV.esm has `fFatigueBlockBase` and
  `fFatigueBlockMult` and no other `Fatigue` setting, and no setting with `Unconscious`
  or `Essential` other than the text [observed 2026-10-17, `fnvtool esm inventory`].
- **Animations** [observed 2026-10-17, IDLE records and the BSA listing]:
  `KnockedDownIdles` (0x00003BD8, `Characters\_Male\IdleAnims`, conditions on functions
  391 and 107 — GetKnockedState) holds `GetUpFaceUp` (0x00003BD9,
  `MT_KnockDownFaceUp.kf`, condition IsFacingUp == 1) and `GetUpFaceDown` (0x00003BDA,
  `MT_KnockDownFaceDown.kf`, IsFacingUp == 0). The two clips are the sequences
  `SpecialIdle_KnockDownFaceUp` / `…FaceDown`, 2 s, clamped; the accumulation root
  `Bip01` has no track and `Bip01 NonAccum` carries the body from the ground to standing
  [observed: `fnvtool nif anim` / `nif decode`]. Creatures have their own:
  `idleanims\specialidle_getupfaceup.kf` / `…facedown.kf` (ghoul, mirelurk, behemoth,
  maintenance robot), `idleanims\mtspecialidle_getupright.kf` / `…left.kf` (dog,
  brahmin). `NPCUnconscious` (0x00060C7D, condition GetUnconscious == 1) and the
  `…Unconscious` creature idles are folders without a clip.

## 2. Rules (`crates/fnv-game/src/unconscious.rs`)

1. A knockout is a state of the reference (`RefExtra::knocked_out`) with a cause,
   essential or fatigue, and the game time it began. It is separate from SetUnconscious's
   flag (`RefExtra::unconscious`), which only scripts set and GetUnconscious reads (§1).
   The player is never knocked out [design: not asked for; the gloves' script spares the
   player anyway], but is knocked down (§5.4).
2. **Essential at zero health**: a hit or a harmful effect that would bring an essential
   actor's health to 0 leaves it at 1 and knocks it out (cause essential). It does not
   die: no Death line, no kill, no XP, no dead flag, no OnDeath, no loot [documented: §1
   GetDead; design: the rest]. KillActor on an essential actor knocks it out too
   [documented: KillActor notes]. It says its KOed line [design: the topic's text names
   fatigue only].
3. **Fatigue at zero**: whenever an actor's Fatigue is lowered (DamageAV, a fatigue
   effect) and its current Fatigue is 0 or less, it is knocked out (cause fatigue) and
   says its KOed line [documented: §1 Stats Tab and KOed].
4. **Fatigue returns** by `FATIGUE_RETURN` = 10 points a second of game time for every
   actor whose Fatigue is damaged [design: no setting in the data, §1].
5. **Getting up** [owner rules 2026-10-17]:
   - A **fatigue** knockout ends after `FATIGUE_DOWN` = 30 s of game time, whether or
     not a fight goes on [owner rule; design: 30 s is a test value — the data has no
     setting for it, §1]. The actor gets up with its Fatigue whole.
   - An **essential** knockout lasts at least `ESSENTIAL_DOWN` = 10 s of game time
     [design: "a few seconds", §1], and then until no fight goes on around the actor:
     no actor in combat (alive, not knocked out itself, not the knocked-out actor)
     within `FIGHT_RADIUS` = 4096 units in the same interior or worldspace [owner rule;
     design: the radius]. The first time a fight holds it down past its time the engine
     logs `knockout: <name> <id> stays down: a fight goes on near it`. It gets up with
     its health back to `ESSENTIAL_UP_HEALTH` = half its maximum (when it had less) and
     its Fatigue whole [design].
   - An actor that died while down (a non-essential knocked out by fatigue and then
     killed) stays dead.
   - **Getting up is part of the knockout** [owner rule 2026-10-17: NPCs do not move
     until their get-up animation is over]: when its time comes the knockout turns to
     getting up (`KnockOut::rising`) and the actor still counts as knocked out — no
     packages, no walking, no fighting, nobody targets it — until the engine has played
     its get-up clip (`Game::finish_getting_up`), or `GETUP_MAX` = 3 s of game time for
     an actor nobody draws [design: the `_male` clips last 2 s]. GetKnockedState keeps
     its knockout value meanwhile [hypothesis]. Beaten to zero again while getting up, an
     essential actor goes down again.
6. A fatigue knockout of an essential actor that is then brought to zero health becomes
   an essential knockout (its time starts again).
7. While knocked out an actor does not fight, talk or run packages, and nobody targets
   it (§3); further hits still land (health damage on a fatigue knockout; an essential
   stays at 1).
8. **Scripts**: GetKnockedState is −1 for a fatigue knockout, 1 for an essential one or
   SetUnconscious [hypothesis for SetUnconscious, kept from before], else 0; GetUnconscious
   reads only SetUnconscious; GetDead is 0 for a knocked-out essential; IsFacingUp and
   IsLeftUp read the side the engine saw the body come to rest on (§4), 1 before that
   [hypothesis]. ResurrectActor ends a knockout as well. GetOwnerLastTarget is the actor
   the calling item's holder (or the calling actor) last hit [hypothesis: the hit target
   is the "target" the summary means].
9. **Weapon OnHit**: when an actor hits another with a weapon whose script has an OnHit
   block, that block runs on the weapon (its instance per holder and item, as the other
   item blocks), after the target's OnHit and OnHitWith [hypothesis: the gloves' script
   only works so; the order is a guess].

## 3. AI

- `Game::would_attack` already refuses an unconscious (or knocked-out) actor on either
  side; detection and awareness forget a knocked-out target as they forget a dead one
  (`Game::settle_awareness`), so fighters turn to other targets and come back to it only
  once it is up and detected again [design, owner request].
- Crime witnesses, aggression (`aggro.rs`) and the AI's movement (`fnv-ai` carry.rs)
  treat a knocked-out actor as they treat SetUnconscious's.
- Dialogue: `Game::start_dialogue` refuses a knocked-out or SetUnconscious actor; the
  player's activation shows "<Name> is unconscious." in the corner (the GMST's text after
  the name) [hypothesis: what the setting's name says it is for].

## 4. On screen

- **Corner message**: an essential knockout shows "<Name> is unconscious." at the top
  left (`sNoTalkUnConscious` after the name; "is unconscious." without the masters). A
  fatigue knockout shows nothing [owner request]. One log line per knockout and per
  getting up: `knockout: <name> <id> knocked out (essential|fatigue)`,
  `knockout: <name> <id> gets up`.
- **Falling**: exactly as if it were killed: the same ragdoll drop as a death
  (ragdoll.md; `engine/nvforever/src/ragdoll.rs`, the `Corpse` deaths.rs gives a dead
  actor), not a knockdown clip; a skeleton without a ragdoll plays its death pose (or is
  laid on its back) as deaths.rs does for the dead [owner rule 2026-10-17].
- **Lying**: it stays a ragdoll while unconscious; the ragdoll rests; the engine reads the side the body lies on from the
  ragdoll's pose — face up when the body's front (left upper arm → right upper arm
  crossed with pelvis → head, signs as in the bind pose) points up — and tells the game
  (IsFacingUp) [design].
- **Getting up**: the ragdoll leaves the physics world and the get-up clip of that side
  plays once from its first frame (`mt_knockdownfaceup.kf` or `…facedown.kf` from the
  skeleton's `idleanims` folder, the creature names of §1 for creatures). The actor is
  first turned and moved so that the clip's first frame lies where the ragdoll lay:
  pelvis on pelvis, the pelvis → head line along the ragdoll's [design]. Without a clip
  the actor simply stands again. After the clip the actor walks and fights as before.

## 5. Knockdowns: PushActorAway, explosions and SetUnconscious

Live actors knocked down without being knocked out (row 1 of docs/outstanding.md §1).
Code: `crates/fnv-game/src/unconscious.rs` (`Game::push_actor_away`, `Game::knock_down`),
`crates/fnv-game/src/functions/fighting.rs` (PushActorAway, GetKnockedState),
`engine/nvf-play/src/combat.rs` (explosions), `engine/nvforever/src/knockouts.rs` and
`ragdoll.rs` (the body thrown).

### 5.1 What the documentation and the scripts say

- **PushActorAway** `source.PushActorAway target force` "knocks down (ragdolls) one actor
  and pushes it away from another with a given force"; a negative force pulls the target
  towards the source; no calling reference launches the target "straight up"; it does not
  work on dead actors, on actors "flagged as immobile", or on actors with the "No
  Knockdowns" flag; "an actor that has been pushed can have its inventory accessed by the
  player until the actor gets up again" [documented: GECK wiki PushActorAway, owner export
  `C:\fnv-private\geckwiki\functions.xml`].
- **GetKnockedState** is 2 for "knocked down", but "will return 0 if called on actors
  knocked down by PushActorAway", and an essential actor knocked out before it recovers
  from a push still reads 0 [documented: GECK wiki GetKnockedState notes].
- **Explosions**: EXPL flags "Knock Down - Always" and "Knock Down - By Formula", and the
  Explosion Knockdown Formula with its seven settings [documented: GECK wiki; already in
  `fnv_combat::damage::knockdown_chance`].
- **No Knockdowns** is ACBS flag bit 26 of NPC_ and CREA, **Immobile** bit 23 of CREA
  [documented: xEdit flag names, `schemas/fnv/gameplay.json`]; template "Base Data" does
  not carry No Knockdowns [documented: GECK wiki Category:NPC, Template Data].
- **SetUnconscious** turns "conscious behavior" off: "Actors that are unconscious will not
  use AI behavior" [documented: GECK wiki SetUnconscious]. In FalloutNV.esm it is called
  107 times by 66 scripts and result scripts [observed 2026-10-18, `fnvtool script
  decompile`]: robots standing in pods (SCPT 0x0005BB12: `SetUnconscious 1` OnLoad,
  cleared when activated), scheduled sleepers (SCPT 0x00159A99), and the thugs that fake
  death (SCPT 0x00118A77 and the trigger SCPT 0x00118A7C: `SetActorValue Fatigue -1`,
  `SetUnconscious 1`, then `PushActorAway <thug> 1`; OnHit sets a variable and, once
  `GetKnockedState == 0`, `SetUnconscious 0`). So SetUnconscious alone must not lay an
  actor down; the lying comes from the push [hypothesis drawn from those scripts].
- **PushActorAway** is called 21 times by 18 scripts, forces 1 to 20, six of them on the
  player [observed 2026-10-18, the same decompile].

### 5.2 Rules

1. A knockdown is a knockout (§2) with cause **pushed** (PushActorAway) or **knocked
   down** (an explosion). It lasts `KNOCKDOWN_DOWN` = 3 s of game time, fight or not,
   then the actor gets up as in §2.5 (getting up stage, get-up clip) [design: no setting
   in the data names it]. No KOed or Wake line, no corner message, no fatigue or health
   restored [design: a knockdown is no unconsciousness].
2. Refused (nothing happens) for dead actors, actors whose own record has No Knockdowns,
   and Immobile creatures [documented: §5.1], and for the player on furniture or in a
   conversation [design]; the player is knocked down otherwise (§5.4).
3. **Order**: an essential knockout replaces a fatigue one, and either replaces a
   knockdown (its time starts again); a knockdown never replaces a knockout. Knocked again
   while getting up: down again (unless only out of fatigue again, §2) [design].
4. **The push**: the body starts with a velocity of `PUSH_SPEED` = 30 game units a second
   per point of force, along the ground from the source to the target (towards the source
   for a negative force), plus `PUSH_LIFT` = 0.25 of that upwards; without a source, or
   with the source on the target, straight up [documented: the direction rules, §5.1;
   design: the numbers — the wiki's "Havok force" is not converted]. A push on an actor
   already lying throws its body again and marks it pushed. An explosion throws the body
   away from the blast at `BLAST_KNOCKDOWN_SPEED` = 300 units a second plus the same lift
   [design].
5. **Scripts**: GetKnockedState reads 0 while an actor pushed down lies, also once
   knocked out meanwhile; 2 for an explosion's knockdown; the other values as §2.8
   [documented: §5.1]. GetUnconscious still reads only SetUnconscious.
6. **SetUnconscious holds a lying actor down**: while the flag is set, no knockout or
   knockdown gets up, whatever its time; once cleared, it gets up at its next frame if its
   time is past [hypothesis: the fake-death thugs, §5.1]. SetUnconscious alone lays nobody
   down (pods, sleepers) [hypothesis, same scripts].
7. **Searching**: activating an actor that lies pushed down opens its inventory, as a
   corpse's does, until it begins to get up [documented: §5.1].

### 5.3 On screen

The body falls as §4 says (ragdoll, else the death pose); the ragdoll starts with the
knockdown's velocity (`Game::take_shove`, `Corpse::velocity`); a push on a body already
lying shoves the ragdoll at its pelvis (`ActorPush`). Log lines: `knockout: <name> <id>
knocked down (pushed|knocked_down)`, `knockout: <id> thrown at (x, y, z) units/s`, the
explosion's `…, knocked down` in its `combat: blast` line, and the exit count
`knockouts: … N knocked down, …`.

### 5.4 The player knocked down

Built 2026-10-18 (claude/nav-knockdown). Code: `crates/fnv-game/src/unconscious.rs` (the
rules), `engine/nvforever/src/player_knockdown.rs` (on screen), `engine/nvf-play/src/script.rs`
(the controls).

1. **Rules**: PushActorAway and an explosion's knockdown lay the player down as anyone
   (§5.2: the push's velocity, `KNOCKDOWN_DOWN`, GetKnockedState 0 pushed and 2 for a
   blast), unless the player sits on furniture or is in a conversation [design]. A blow
   that would knock the player out (essential, fatigue) does not (§2.1). Six shipped
   PushActorAway calls throw the player (§5.1) [observed].
2. **Enemies** may attack the player while down: `Game::would_attack` keeps a knocked-down
   player a target, where it drops a knocked-out NPC (§3) [design; how the original's
   attackers treat a downed player is not in our notes].
3. **Controls**: all held (movement, looking, fighting, the Pip-Boy, POV) from the fall
   until the get-up idle is over; log `player controls: all held, the player is knocked
   down` [design].
4. **The body**: the third-person rig drops as its ragdoll, thrown with the knockdown's
   velocity, as the dead player's does (`death_cam.rs`); a push while it lies shoves it
   again; the side it comes to rest on is told to the game (IsFacingUp, §4's rule).
5. **The view**: in third person the camera keeps its place against the body's chest and
   turns to look at it, easing after it and kept in front of walls (the death camera's
   rule). In first person the view goes to third person for the knockdown (the camera 170
   units behind and 60 above the chest) and back to first person when the player is up
   [hypothesis: the original's first-person knockdown; capture job `kd_player_knockdown`].
6. **Getting up**: when the rules' knockdown time is over (`KnockOut::rising`) the ragdoll
   leaves the world, the character stands on the ground under the ragdoll's pelvis, and the
   IDLE `GetUpFaceUp` or `GetUpFaceDown` (§1) is asked for on the player as PlayIdle would
   (`Game::ask_idle`), so `idles.rs` plays its clip on the third-person body. Two frames
   into it the character is turned and moved so that the clip's pelvis and pelvis → head
   line lie where the ragdoll's did (the NPCs' rule, §4); when the idle has played (or
   after 4 s of real time) the rules are told (`finish_getting_up`), and the controls and
   the view are the player's again. Without a ragdoll the player stays where they are
   until the knockdown's time is over.
7. **Death while down**: the death camera takes over (its own ragdoll drop).

Log lines: `player knockdown: the player falls (<cause>, <first|third> person[, the view
goes to third person]): ragdoll thrown at (x, y, z) units/s`, `player knockdown: the player
gets up face up|down <s> s after the fall (<idle>)`, `player knockdown: laid over the
ragdoll: turned <deg>°, moved <n> units`, `player knockdown: the player is up <s> s after
the fall; the controls[ and the first-person view] are theirs again`, and at exit `player
knockdown: <n> knockdown(s) shown, <n> get-up clip(s)`. Smoke: `knockdown-player`,
`knockdown-player-third`.

Not known (capture job `tools/capture/jobs/kd_player_knockdown.steps.json`, nvfstrip):
whether the original's view leaves first person, where its camera goes, how long the
player lies and how long the get-up takes.

First run of the job (2026-10-18, attended, output `kd_player_knockdown.1792356303.jsonl`)
[observed]: the first-person trial is void: the game window was not in front (every
screenshot `foreground: false`) and the game stood still: GameDaysPassed 15.8393 and the
same position and heading in all 48 samples (the lost-focus pause). The third-person trial
ran (GameDaysPassed 15.8419 → 15.8462, about 12 s at timescale 30), but its 48 screenshots
failed (`BitBlt failed`). The player reference's own position moved only about 16 units
(away from the can's side was +y; it went −x, −y) and was still from about 1.75 s after the
push, its heading turned 7° at about 2.25 s, and GetKnockedState read 0 throughout, which is
what a pushed actor reads (§5.1). So the numbers neither show nor rule out the fall: the
reference may stay with the character while the body is a ragdoll. Nothing was fitted from
them; §5.4's camera and timing stay [hypothesis] until a run with pictures, with the game
window in front for the whole job.
