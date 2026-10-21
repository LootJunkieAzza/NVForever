# Base effect archetypes: Paralysis, Turbo, Concussion, the cures and Dispel

What the base effect (MGEF) archetypes that were not built do when an effect of theirs
starts on a target: Paralysis, Cure Paralysis, Cure Poison, Concussion, Turbo and Dispel.
ESM gap audit row 15 (`docs/handoff/esm-gap-audit-2026-10-21.md`). Implementation:
`fnv_game::aid` (`Game::effect_archetype_start`, `value_change`), `fnv_game::unconscious`
(the paralysis knockdown), `fnv_game::magic_archetypes` (concussion, Turbo, the Paralyzing
Palm), nvf-play `script.rs` (Turbo's clock) and `player.rs` (the player kept at full
speed). The value-changing archetypes and Cure Addiction: aid-items.md §3, §5.4. Evidence
tags as in the charter.

## 1. Sources

- **Archetype codes** [documented: GECK wiki GetBaseEffectArchetype; xEdit
  wbArchtypeEnum, schema `gameplay.json`]: 2 Dispel, 3 Cure Disease, 24 Paralysis,
  30 Cure Paralysis, 31 Cure Addiction, 32 Cure Poison, 33 Concussion, 36 Turbo (with the
  others already built).
- **The masters' base effects** [observed: `fnvtool esm decode` of every MGEF of the six
  masters, 2026-10-21]. Of 425 MGEFs, 237 are Value Modifier and 175 Script. Of the
  archetypes here:
  - Paralysis (4): `Paralysis` 0x0006B20C (Hostile, Recover, Touch, Target, actor value
    Paralysis), `NVMissFortuneParalysis` 0x0013B2C7, HonestHearts `ParalyzeSelf`
    0x0100F7A5, OldWorldBlues `NVDLC03ParalyzeSelfBE` 0x01013EB3. Each names the actor
    value Paralysis (code 47).
  - Cure Paralysis (1): HonestHearts `NVDLC02CureParalysis` 0x0100CCEF. No SPEL, ALCH or
    ENCH of the masters lists it.
  - Concussion (1): `Concussion` 0x00065339 (Self, Touch, Target, No Magnitude).
  - **No MGEF has the archetype Dispel, Cure Disease, Cure Poison or Turbo.**
- **The spells that carry them** [observed: a scan of the masters' EFID subrecords]:
  - `PerkParalyzingPalm` 0x0006B20B "Paralyzing Palm", an Actor Effect: `Paralysis`
    magnitude 1 for **30 s**, type Touch, with one condition (function 438, parameter 6).
    The NPC_ `vFortGladiatorRangerStella` 0x00137123 lists it among its actor effects.
    GetDefaultForms lists "Paralyzing Palm (0006B20B)" among the engine's default forms
    [documented: GECK wiki GetDefaultForms example]; the perk entry point "Has Paralyzing
    Palm" is "Run when the player enters VATS. If the value is modified to be non-zero,
    the player may use the paralyzing palm" [documented: GECK wiki Perk Entry Points],
    and `fVATSParalyzePalmChance` (default 0.3) "is the chance of triggering the
    paralyzing palm effect per hit in VATS" [documented: GECK wiki VATS Settings].
  - `MissFortuneParalysisSpell` 0x0013B2CD: `NVMissFortuneParalysis` 1 for 9 s, Self;
    OldWorldBlues `NVDLC03SonicRevParalysisSpell` 0x010157FD the same with a condition;
    HonestHearts `NVDLC02CompRegParalysisAE` 0x0100F7A3 (10 s); OldWorldBlues
    `NVDLC03AntennaClubParalysisAE` 0x01013EB6 (10 s).
  - `PlayerConcussed` 0x0006533A "Concussion": the `Concussion` effect for ten years and
    Perception −4 while the head is crippled; `GenericScript` casts it on the player
    whenever the player lacks the Concussion effect (crippled-limbs.md §1). The GMST
    `fConcussionTimer` is 40.0 and the IMAD `ImageSpaceConcussion` 0x00000164 (an engine
    form) is animatable with a duration of 11 s [observed].
- **Antivenom** (`NVAntivenom` 0x000E2C6F; also Lonesome Road's Tourniquet and Fiery
  Purgative): its effect `NVAntivenomEffect` "Cure Animal Poison" is a **Script**
  archetype, not Cure Poison. Its script (SCPT 0x000E2C6E) dispels eight spells by name in
  its ScriptEffectStart block (BarkScorpionPoison, CazadorPoison, DeathClawPoison,
  DLC03FeralGhoulPoison, DLC03RadScorpion3Poison, NightstalkerPoison, RadScorpion1Poison,
  RadScorpion2Poison) and shows a message [observed: `fnvtool script decompile`]. All eight
  spells are of SPIT type **Poison** (5) [observed]. So Antivenom works through Dispel
  (scripts.md), not through this note.
- **Turbo** (`NVTurbo` 0x00120D81, a chem): its effect `TurboEffect` 0x00120D80 is a
  **Value Modifier** of the actor value Turbo (code 51), magnitude 1 for 3 s, or 4 s with
  its other entry's condition (HasPerk 0x0015EAD9), Recover, hit sound `FXTurboLP`
  0x00161EB8 [observed]. No GMST names Turbo [observed: the GMST editor IDs].
- **Paralysis is a knockout**: GetKnockedState's 1 "includes unconscious essential actor,
  paralyzed actor" [documented: GECK wiki GetKnockedState]; JIP's ragdoll event fires
  "upon death, paralysis, knockdown, low fatigue, or unconsciousness" [documented: GECK
  wiki SetOnRagdollEventHandler]. The misc statistic "Paralyzing Punches" counts the
  palm's hits [documented: GECK wiki GetPCMiscStat].
- No source says how much Turbo slows time, what a Concussion does or what Dispel and Cure
  Poison take away; those rules below are hypotheses.

## 2. The rules

- **Paralysis** (archetype 24): the effect raises its actor value (Paralysis) by its
  magnitude while it lasts, as a Value Modifier with Recover does [hypothesis: the MGEFs
  name the actor value and carry Recover]; and its target is **knocked down paralysed**
  (`Cause::Paralyzed`: the body goes limp as a ragdoll, it does not move, fight or talk)
  for as long as any Paralysis effect lasts on it, then gets up as from a knockdown
  [documented: paralysis is a knockout and a ragdoll, §1; hypothesis: the length is the
  effect's]. GetKnockedState reads 1 while it lies [documented]. The player is paralysed
  as it is knocked down (unconscious.md §5.4) [hypothesis]. A paralysed actor that is
  knocked out (essential, fatigue) stays down for the stronger cause; a push while it
  lies throws the body again. "No Knockdowns" and Immobile actors are paralysed all the
  same [hypothesis: the flag names knockdowns, not paralysis].
- **Cure Paralysis** (30): ends the target's Paralysis effects [hypothesis from the name].
- **Cure Poison** (32): ends the target's effects that come from a spell of type Poison
  [hypothesis from the name; the eight poisons Antivenom dispels are all of that type, §1].
- **Dispel** (2): ends the target's effects that are not permanent (abilities, worn
  enchantments and addictions stay) and that do not come from the dispelling spell itself
  [hypothesis: the archetype's name; no master uses it].
- **Concussion** (33): an actor with an active Concussion effect whose head is crippled
  (Perception Condition at 0 or below, crippled-limbs.md) is **concussed** for
  `fConcussionTimer` seconds of game time from the crippling: GetConcussed reads 1, and on
  the player the `ImageSpaceConcussion` modifier plays [hypothesis: the effect the player
  always carries is the switch, the timer and the image space modifier are the
  concussion's; the help text names "a concussion effect" for a crippled head]. Each new
  crippling of the head starts the time again. Without the effect a crippled head does
  not concuss (so only the player, through `GenericScript`, in the shipped data).
- **Turbo** (36, and the actor value Turbo that `TurboEffect` raises): while the
  **player's** Turbo is above 0 the game's clock runs at **0.5** of its speed
  (`TURBO_TIME_SCALE`) for everything but the player [hypothesis: the number; design:
  the exemption]: NPCs, physics, animations, projectiles, effects and the game clock slow
  as with SetGlobalTimeMultiplier 0.5, and the player's walking, running and swimming
  speed and its body's clips are multiplied by 2 (the clock's inverse) so the player
  moves as fast as before through the slowed world. Not exempt: gravity and jumps, the
  rate of fire and the reload (weapon timing runs on the game clock), the player's
  effects' durations. Sounds slow with the clock as the engine's own slow motion's do
  (`audio_out::slowed_sound`) [hypothesis]. An actor other than the player with Turbo
  is not sped up [design: one clock].
- **Paralyzing Palm**: a hit of the player's unarmed attack (no weapon, or a weapon of the
  Unarmed skill) while VATS plays back, with the perk entry point "Has Paralyzing Palm"
  (29) above 0, rolls `fVATSParalyzePalmChance`; on success the target gets
  `PerkParalyzingPalm` as a hit effect (the effect machinery checks its condition) and
  "Paralyzing Punches" counts one [documented: the settings and entry point, §1;
  hypothesis: the order, the weapon test].

## 3. Log lines and checks

- `knockout: <name> <ref> knocked down (paralyzed)` and `… gets up (paralyzed knockout
  over)`; `magic: <name> <ref> paralyzed by <effect> for N s`, `magic: <Cure Poison|Cure
  Paralysis|Dispel> on <name> <ref>: N effects ended`, `magic: <name> <ref> concussed for N
  s`, `magic: Paralyzing Palm strikes <name> <ref>` (nvf-play `knockouts.rs`); `magic: Turbo
  on: the world runs at 0.50, the player at full speed` / `magic: Turbo off` (nvf-play
  `script.rs`); `magic: <poison> on the player ended` (nvf-play `aid.rs`, a poison spell's
  effect on the player ending, run out or dispelled).
- Unit tests: `crates/fnv-game/tests/magic_archetypes.rs` (built records, no game data).
- Smoke: `magic-archetypes` in `tools/smoke/scenarios.toml`.
