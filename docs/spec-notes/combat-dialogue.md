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
| Hit | an actor is hit | no timer; said when the damage as a fraction of health is above `fCombatSpeakHitThreshold` and (an NPC or creature; the player: or) a random number in [0, 1) is below `fCombatSpeakHitChance` (from 2026-10-17, below) |
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
`fCombatSpeakAttackChance` 0.01 exists too; the page names no chance for Attack. From
2026-10-17 an NPC or creature needs a random number below it on top of the timer, and a
Hit line needs both the threshold and `fCombatSpeakHitChance`, while the player keeps the
timer alone and either condition [hypothesis fitted to the original: in its NPC battles
about 3 Attack-like lines for some 175 attacks begun and about 1 % of 217 hits followed by
a line; ours said 47 combat lines in one 32-second battle, 26 of them Hit;
npc-behaviour-compare.md §2].

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

## 5. Which situation says which topic (audit 2026-10-17, todo 6 item 1)

The owner heard Violet say "You're lucky I didn't find you." after she killed the player
and took it for a Lost line. That line is GenericFiend's line 0x00175190 of
**CombatToNormal** [observed: `fnvtool dialogue lines --topic CombatToNormal --text`,
FalloutNV.esm], conditioned only on the speaker's voice type; GenericFiend's other two
CombatToNormal lines are "That's right, run." and "Fucking chicken." So Violet said the
right topic: the data's Fiend lines for the end of a fight are written as if the target
ran, while GenericAdultCombat's ("Teach you to mess with me.") and the robots' read as a
win [observed, same listing]. A fight whose target died ends in Normal: "Combat → Normal
… whenever I have no targets in my combat list (i.e. they're all dead or no longer
considered hostile), and I have nothing in my alert list" [documented: GECK wiki
Category:Detection, owner export `Actor-Behavior.xml`]. No topic of the masters is meant
for "my target died" [observed: the 55 Combat and Detection topics of FalloutNV.esm].

What was wrong: a dead **player** was forgotten only when the engine measured it again
(`Game::observe_threat` drops a target the actor no longer would attack); the clean-up
in `Game::settle_awareness` that forgets dead and gone targets skipped the player. When
no measurement came (detection off, the test aid that hides the player), a fighter kept
the dead player as an undetected target and went to Lost after `Timers::lost`, saying
CombatToLost, then LostToNormal. Since 2026-10-17 a dead player is forgotten there too.
Tests: `crates/fnv-game/tests/combat_transitions.rs`; smoke `combat-target-killed`.

Every Combat and Detection topic of FalloutNV.esm and what says it in NVForever
(meaning: [documented: GECK wiki Combat Tab, Category:Detection], unless marked):

| Topic | Meaning | Said by NVForever |
|---|---|---|
| NormalToAlert, NormalToCombat, AlertToCombat, AlertToNormal, CombatToNormal, CombatToLost, LostToNormal, LostToCombat | the change of detection state the name gives | `awareness::State::topic_to` on every change ([`Game::settle_awareness`]); matches the documented transitions; a dead target ends Combat in Normal (above) |
| AlertIdle, LostIdle | now and then while searching | `State::idle_topic` |
| Attack, PowerAttack, FireExplosive, Hit, Death, DeathResponse, Crippled, HealthHalf, HealthQuarter, WeaponBroke, NoAmmo | §1 | §1 (`combat_lines.rs`, engine hooks) |
| Flee | an actor begins to flee | `combat_group.rs` when a flight begins |
| Assault | first attacked | `provocation.rs`, `combat_group.rs`, `crime.rs` (SendAssaultAlarm) |
| AcceptYield | accepts a yield | `provocation.rs` |
| Steal, Murder, GuardTrespass | a witness of theft or murder; a trespass or aggro-radius warning | `crime.rs` (`Kind::topic`), `aggro.rs` |
| KOed | loses all fatigue and falls unconscious | `unconscious.rs` (also an essential actor at 0 health, design) |
| Wake | wakes from unconsciousness | `unconscious.rs` when it begins to get up (added 2026-10-17; before, never said) |
| AssaultNoCrime, MurderNoCrime, StealNoCrime, TrespassNoCrime, PickpocketNoCrime | as Assault, Murder … when the actor does not count it a crime | not said: the crime code says the crime topics only for crimes (follow-up) |
| Pickpocket | notices the player picking its pocket | not said: pickpocketing is not built |
| Yield | yields to whom it fights | not said: NPCs do not yield |
| AvoidThreat, AvoidThreatResponse | begins avoiding a threat ("It's going to blow!"); per-group timer | AvoidThreat said once per grenade a fighter runs from (combat-ai.md §17.7; no per-group timer); AvoidThreatResponse not said |
| AttackResponse, FleeResponse, FireExplosiveResponse | a valid actor nearby answers that line | not said (follow-up; DeathResponse is) |
| StartCombatResponse, NormalToAlertResponse | (Detection; not on the pages) | not said [hypothesis: answers to NormalToCombat/NormalToAlert, as the other Response topics] |
| ArmorIneffective, WeaponIneffective, OnFire, Poisoned, KnockedBack, UsedDoctorBag, Regenerating, Frenzy | the situation the name gives | not said (follow-up) |
| CombatIdleChatter, GroupStrategy, DeathResponseUnderworld, Stea | "Unknown" / "Unused" | not said |
