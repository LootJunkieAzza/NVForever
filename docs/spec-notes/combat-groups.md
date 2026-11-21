# Spec notes: combat groups (alarm reach, whom joiners fight, shared targets)

How the actors of one side of a fight help each other: who hears an alarm, whom an
alarmed actor is sent against, and how long a group goes on fighting a target only some of
its members see. Code: `crates/fnv-game/src/combat_group.rs` (`Game::raise_alarms`,
`Game::first_starter`, `Game::share_sightings`) and `crates/fnv-game/src/awareness.rs`
(`Awareness::settle`). The rest of the combat groups (threat ratios, flight, searches) is
docs/design/combat-ai.md §12; issue #972.

What the original does is from one observer run of the Goodsprings gunfight and one at
Cook-Cook's camp, with the JIP functions [observed: docs/handoff/observer-jip-audit-2026-10-25.md
§3.3, §3.5, §6 item 5]. Combat mechanics are not a parity target (owner: modern design is
allowed); what matters for quests is that townsfolk and animals join a fight on their
town's side.

## 1. Who hears an alarm

- **The original** [observed: §3.5]: in the Goodsprings gunfight an actor's combat allies
  stood a median 1,044 units from it, 4,864 at the 95th percentile and up to 9,026
  (19,752 pairs).
- **NVForever** [design]: an actor that starts fighting or notices a hostile alarms
  comrades within `ALARM_RADIUS`, **9,216 units** (the farthest observed pair rounded up
  to a quarter cell; 2,048 until playtest list 5 item 31, 4,096 until #972).
- Hearers are the actors measured in that detection round (drawn, in the high process)
  **and** the other actors of the loaded cells, at the position the world keeps for them
  [design: the original's allies reached past the actors near the player; our engine
  measures only drawn actors]. Who among them answers is unchanged (combat-ai.md §12.3:
  its own kind, or an ally or friend whose Assistance helps the sender; never against its
  own ally or friend; a coward stays out).

## 2. Joiners fight the first of the foes to start

- **The original** [observed: §3.5]: the first OnStartCombat events were Gangers 3, 6, 2
  and 5 against Sunny; every townsperson's own OnStartCombat then named **GSPG03Ref**, the
  first Ganger to start, whichever Ganger the sender was fighting.
- **NVForever**:
  - Each fighter's first move to Combat in a fight is remembered (`Tactics::started`,
    forgotten when it calms) [design].
  - The first starter of a foe's side is the member of the foe's combat group (its
    comrades in a combat state with a foe in common) that went to Combat earliest, ties by
    FormID; the foe itself when none of them is fighting (the player, for one)
    [design on the observation]. A member dead or out cold (knocked out, or set
    unconscious) is passed over: the next to start is named [design].
  - An alarm names that first starter, at its position when it was measured (or where
    the world has it in the loaded cells), instead of the sender's own foe, unless the
    starter is the hearer or the hearer's ally or friend (then the sender's foe, as
    before) [design].
  - A hearer that would not attack them otherwise (a neutral helping by its Assistance)
    holds an Assist grudge against both the sender's own foe and the starter, so it can
    still fight the foe in front of it once the starter is gone [design].
  - An actor alarmed by an ally (`Cause::Ally`) that is not fighting anyone yet takes the
    alarm's owner as its target once it detects it, and holds it for the target
    commitment (alert.md, `TARGET_COMMIT`), even when it detects another foe better
    [design: so its fight is against the starter, as the townsfolk's was]. Until it
    detects the starter it fights whatever it detects best, as before.

## 3. A group keeps a target while a member detects it

- **The original** [observed: §3.3]: at Cook-Cook's camp five Fiends stayed in combat with
  the hidden player for the whole half hour, kept it as their combat target and closed
  from 1,466 to about 110 units, though only 0 to 5 of them (often 1 or 2) detected it at
  any moment.
- **The reading built here** [hypothesis, not yet captured: observer-jip-audit §6 item 5
  says the "some member detects" reading is unconfirmed; observer-checks.md has the
  question]: a fighter neither loses nor forgets a target while a member of its combat
  group still detects it.
- **NVForever**:
  - Before each round's settling, every fighter in Combat learns, for each target it does
    not detect now, the latest time a comrade in Combat with the same target detected it
    and where (`Awareness::shared`; the target's last known place moves there, so the
    fighter closes in on it) [design].
  - The Lost timer (10 s) runs from the later of its own last detection and the group's;
    the forget timer (15 s) keeps a hidden target on its list while the group detected it
    within that time [design on the hypothesis].
  - It does not count as seeing the target: it fires only at what it detects itself
    [design].
  - When nobody in the group detects the target any more, every member goes Lost the Lost
    timer after the group's last sighting, together.

## 4. A faction Friend that Helps Friends and Allies joins a fight against the player

Issue #2927: a player fought mole rats beside Gibson's scrap yard and the companion then
killed Gibson's dogs.

- **The records** [observed: FalloutNV.esm, `fnvtool actor factions`]: GibsonDog01-06 and
  Rey (template CrDog) are Aggressive and Helps Friends and Allies; one of their factions,
  VCrVoidCritterFaction, names MoleRatFaction a Friend. To the player they are Neutral.
- **The original** [observed, base game, three runs of one observer job, 2026-11-17]: the
  player (god mode, never attacking) stands 665 to 1,640 units from four of the dogs
  (0x0008E447, 0x0008E44C, 0x0008E44D, 0x000CD22B) outside the yard; nothing happens for
  300 frames. Three mole rats (base 0x0001CF9F) are placed 300 units from the player and
  attack it.
  - Runs 2 and 3: within 60 to 180 frames the dogs are in combat with the player as their
    combat target, three of four in run 2 (the fourth, 1,300 to 1,400 units off, never)
    and all four in run 3, before any mole rat is harmed. `GetShouldAttack player` reads
    0 on every dog throughout: it is the mole rats' fight they take up, not their own
    aggression.
  - The mole rats are then killed one by one (`Kill player`). The dogs stay in combat with
    the player, 80 to 200 units from it, to the end of the run (about 1,400 frames after
    the last mole rat died).
  - Run 1: no dog joined in 12 samples; the player's own combat ended after the fifth, so
    the mole rats lost the player early (the run did not count them). Whether a dog joins
    follows from whether it notices the fight.
- **NVForever**: the same, by two paths: a dog that detects a mole rat fighting the player
  takes an Assist grudge against the player (`Game::call_allies`), and a mole rat that
  starts fighting alarms the dogs in reach (§1). The grudge outlasts the mole rats, as the
  original's combat does. A companion then fights the dogs because they attack the player
  [documented: GECK wiki, SetPlayerTeammate]. Nothing was changed for #2927; the test
  `a_faction_friend_that_helps_friends_joins_a_fight_against_the_player` holds the rule.
- Not measured: how far the original's dogs hear it (the one dog that stayed out was the
  farthest, past 1,300 units, in run 2, and joined from 1,335 in run 3), and when their
  combat with the player ends.
