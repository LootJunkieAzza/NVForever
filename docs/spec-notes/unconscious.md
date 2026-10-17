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
   player anyway].
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
