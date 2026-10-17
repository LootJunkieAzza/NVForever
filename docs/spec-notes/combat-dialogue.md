# Spec notes: Combat-tab dialogue (combat lines and effort sounds)

What actors say in a fight: the topics of the quest window's Combat tab (DIAL type 2,
docs/spec-notes/dialogue.md §1). Code: `crates/fnv-game/src/combat_lines.rs` (when a line
is said), the engine hooks listed in §4, and `engine/nvf-play/src/voice.rs` (the response's
own sound, §3). Detection-tab lines (NormalToCombat …) are docs/design/alert.md's; this note
reuses their way of saying a line (`Game::detection_bark`).

## 1. The topics and when they are said

The game says these topics by itself; other Combat-tab topics are said only when a script
or a link names them [documented: GECK wiki Combat Tab, owner export
`C:\fnv-private\geckwiki\Combat.xml`]. The ones NVForever says, and what the page says
about each:

| Topic | Said when [documented: Combat Tab] | Timing [documented unless marked] |
|---|---|---|
| Attack | an actor begins an attack | per actor: after an Attack or PowerAttack line, no other for a random time between `fCombatDialogueAttackMinElapsedTime` and `…MaxElapsedTime` |
| PowerAttack | an actor begins a power attack | as Attack (one timer for both) |
| Hit | an actor is hit | no timer; said when the damage as a fraction of health is above `fCombatSpeakHitThreshold`, or a random number in [0, 1) is below `fCombatSpeakHitChance` |
| Death | an actor dies | always |
| Crippled | one of its limbs is crippled | none given [design: always] |
| HealthHalf, HealthQuarter | at half, at a quarter of its health | none given [design: once per crossing, §2] |
| WeaponBroke | the weapon it uses breaks | none given [design: always] |
| NoAmmo | it runs out of ammunition | none given [design: always] |
| FireExplosive | it throws a grenade or fires an explosive | none given [design: as Attack's timer, and it takes the turn of that attack's Attack line] |
| Flee, Assault, AcceptYield, GuardTrespass | (already said before this note: provocation.rs, combat_group.rs, crime.rs) | unchanged |

The game settings in FalloutNV.esm [observed 2026-10-16, `fnvtool esm decode`]:
`fCombatSpeakHitThreshold` 0.01, `fCombatSpeakHitChance` 0.01,
`fCombatDialogueAttackMinElapsedTime` 5, `fCombatDialogueAttackMaxElapsedTime` 30.
`fCombatSpeakAttackChance` 0.01 exists too; the page names no chance for Attack, and a 1 %
chance per attack would make the lines rare [design: not used; the timer alone decides].

"Health" for Hit and for HealthHalf/Quarter is the actor's base health (the Health actor
value with no damage) [hypothesis: the page says "damage %" without saying of what].

## 2. Rules of our own [design]

- A combat line is said like a Detection line (`Game::detection_bark`): not while a
  conversation with the player or a menu holds the game, not while the actor is already
  saying a line, and never by a dead actor except its Death line. Death cuts whatever the
  actor was saying.
- Hit is not said by an actor that is saying an Attack/PowerAttack/Hit line (it would cut
  its own grunt on every pellet of a shotgun). The other way round, an Attack, PowerAttack
  or FireExplosive line cuts the actor's own Hit line when the actor has a line to say
  (otherwise a blow taken just before a power attack silences the power attack's grunt;
  observed 2026-10-17 in the combat-lines smoke run with voice on). Every other combat
  line cuts nothing.
- HealthHalf and HealthQuarter: said when a hit takes health from above the mark to at
  or below it; the Hit line is not said for that hit. Each is said again only after health
  rose above the mark.
- The cooldown of Detection lines within a group (owner request 2026-10-11, alert.md) is
  not applied to combat lines: every actor of a group grunts when hit.
- The player says combat lines as any actor does: the masters give the player lines of
  their own (§3).

## 3. Lines in the data [observed 2026-10-16, FalloutNV.esm, `fnvtool dialogue`]

- 43 Combat topics; Attack has 249 lines, Hit 156, PowerAttack 59, Death 93.
- Lines are chosen as any line (quest priority, conditions, Random; dialogue.md §4). Most
  are conditioned on `GetIsVoiceType` or `GetIsID` of the speaker.
- **The player:** quest `GenericPlayer` (conditioned on `GetIsID Player` = 1, the player's
  voice type is `PlayerVoiceMale`) has Hit, PowerAttack and Death lines with voice files
  (`sound\voice\falloutnv.esm\playervoicemale\genericplayer_hit_…`: 32 Hit, 12
  PowerAttack, 14 Death files counted as `.ogg`).
- **Creatures and robots:** 15 Hit lines and 3 Death lines have a response sound (TRDT
  sound) and no voice file: creatures by faction (`CreatureHitBrahmin`, `…Ant`, `…MoleRat`,
  `…Dog`, `…BloatFly`, `…DeathClaw`, `…Scorpion`, `…QueenAnt`, `…YaoGuai`), robots by voice
  type (`RobotProtectron`, `RobotVictor`, `RobotMisterGutsy`, `RobotRoboBrain`): e.g.
  `NPCBrahminInjured`, `NPCAntInjured`, `FXExplosionRobotBuzz`, `FXExplosionRobotDeath`.
  A response's sound "plays instead of the voice" [documented: Category:Dialogue,
  "Sound"]: the engine plays that SOUN where the speaker stands and no voice file.

## 4. Where the engine says them

See `combat_lines.rs`'s module comment for the list of hooks (hits, attacks, power attacks,
deaths, crippled limbs, broken weapons, empty magazines, grenades).
