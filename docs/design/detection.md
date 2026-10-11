# Detection design: who perceives whom

Owner request 2026-10-11: NPCs and creatures detect the player and each other, from
Perception and the target's Sneak when it sneaks, with the numbers that decide hidden or
detected in place. Combat encounters will be driven from it later; this step is the
basics. Facts and their sources: docs/spec-notes/actor-behaviour.md §2 (the GECK wiki
Category:Detection page in the owner's export, and the script catalogue notes).

Tags as in the spec notes: [documented: source], [observed], [hypothesis], and
[engineering choice] for what NVForever decides where the original's rule is unknown and
parity does not matter.

## 1. What is built

| Part | Where | What |
|---|---|---|
| The formula | `fnv_game::detection` | `Settings` (the 31 game settings with their documented defaults, read from the load order), `Pair` (what the two actors' records and actor values say), `Situation` (what the world shows), `Settings::evaluate` → `Detection { value, band, sound, visual, detector_skill, target_skill, … }` |
| Bands | same | hidden (≤ −20, fSneakNoticedMin), noticed (−20 < D ≤ 0), detected (> 0) [documented] |
| The table | `Game::detection` | the latest value of every (detector, target) pair the engine measured; not saved |
| Script functions | `fnv_game::detection` | `GetDetected` (band detected), `GetDetectionLevel` (0 not measured, 1 hidden, 2 noticed, 3 detected) |
| Armour class | `fnv_sim::forms::Item::armor_class` | ARMO BMDT general flags Medium and Heavy [documented: xEdit] |
| The engine | `engine/nvforever/src/detection.rs` (`--play`) | four times a second every living drawn actor within reach of the player measures the player and the other actors within its reach; logs band changes and the player's highest band; `detection summary` at exit |
| Numbers without the game | `fnvtool actor detect` | §4 |

## 2. The formula

    D = fSneakBaseValue + Sound + Visual + fSneakSkillMult · (DetectorSkill − TargetSkill)

exactly as actor-behaviour.md §2.1 gives it [documented: GECK wiki Category:Detection].
The doc comment of `fnv_game::detection` spells out every term. Two readings the page
leaves open:

- A running target also counts as moving in the visual movement multiplier
  (1 + 0.01 + 0.2) [hypothesis].
- The distance falloff is 0 at and beyond the reach, so only the base and the target's
  skill remain there: a target out of reach is hidden (−35 or lower).

## 3. What the engine measures

Each is also listed in the module notes of `detection.rs`.

| Input | How | Tag |
|---|---|---|
| Distance | feet to feet | — |
| Reach | fSneakMaxDistance, × fSneakExteriorDistanceMult outside | [documented] |
| Line of sight | a ray through the collision world from the detector's eyes to the target's chest (¾ of its eye height); world shapes block it, actors and their capsules do not | [engineering choice] |
| Field of view | 95° either side of where the actor's body faces, horizontally | [documented: catalogue notes, GetLineOfSight]; using it for detection: [hypothesis] |
| Light at the target, 0–100 | 100 × (luminance of the ambient colour + the sunlight's × daylight outdoors, or the cell's directional light indoors + each point or spot light that reaches the target × (1 − (d/r)²)), capped at 100. No shade from the sun | [hypothesis] |
| Moving, running | from the distance moved since the last measurement: moving above 10 units/s; actors run above 215 units/s; the player runs when moving without holding Walk, sneaking or not | [engineering choice]; sneak-running counts as running: [hypothesis] |
| Sneaking | the player's sneak toggle; actors never sneak (Always Sneak packages are not acted on) | — |
| Action sound | the player's shots since the last measurement, at the weapon's VNAM sound level (100 / 50 / 10) | [documented] |
| Detector asleep | lying on a bed (marker 1–10, state "on it") | [hypothesis] |
| Detector alert or busy | never: there is no combat AI yet | — |
| Interval | 0.25 s | [engineering choice] |

Collision is loaded only around the player, so far from the player nothing blocks sight
between actors. Pairs not measured for 1 s of game time are dropped from the table.

## 4. `fnvtool actor detect`

```
fnvtool actor detect --schema schemas/fnv <Data> FalloutNV.esm --detector SunnyRef [--target player] [--interior] [--light 90] [--dark 10]
```

For one pair in the authored game, `fnv.actor.detect/1`: the settings, the inputs, and
for 13 situations (standing, walking, running, lit and dark, sneaking, behind the
detector, behind a wall, firing) the value and band at 100–5,000 units and the farthest
distance at which the target is detected and noticed.

Sunny Smiles (Perception 5, level 2) against a level-1 player with Sneak 15, outdoors,
lit 90, dark 10 [observed: 2026-10-11]:

| Situation | Detected out to | Noticed out to |
|---|---|---|
| standing, lit | 2,590 | 3,420 |
| running, lit | 2,940 | 3,650 |
| standing, dark | 260 | 1,890 |
| sneaking, lit | 1,840 | 2,420 |
| sneaking, dark | never | never |
| sneaking and walking, dark | never | 850 |
| behind a wall, walking | never | 1,260 |
| sneaking, firing a normal gun, lit | 2,800 | 3,200 |

The light level decides most of the visual term, and its measure is a hypothesis
(§3): it is the first number to tune against the game (§6).

## 5. Checks

- Unit tests: `fnv_game::detection` (the formula's terms, bands, the table) and the
  engine's field of view and light sum.
- In the game (`--play`, Goodsprings at noon, with `NVF_PLAY_TEST_SCRIPT_MENUS=1
  NVF_PLAY_TEST_EQUIP=000E3778 NVF_PLAY_TEST_GIVE=0008ED03:50
  NVF_PLAY_TEST_CONTROL=10:sneak,14:weapon,17:reload,22:fire,22.2:fire`):
  `nvforever --data <Data> --cell Goodsprings --play --hour 12 --no-voice --run-seconds 28`
  logs the settings line, band changes such as `detection: GSSettlerAM 00104F0A → the
  player: hidden → noticed (value -10.4: … skills 16.8 against 55.0; 2104 units, in
  sight, light 93, sneaking)`, the shot taking that settler to detected (sound 55.3),
  the player's meter (`the player is DANGER (NVCrRaven 0014C447: 43.3)`) and the
  `detection summary` line [observed: 2026-10-11].

## 6. Next and open

- **Detection states** (Normal, Alert, Combat, Lost), the target list and its timers,
  and detection events (CreateDetectionEvent, projectile impacts, explosions):
  actor-behaviour.md §2.2–§2.3. They need hostility (`fnv_game::rest::hostile_to_player`
  generalised to any two actors) and are the entry into combat AI. Until then bands can
  flicker as an actor turns; the states' timers smooth that in the original.
- **Captures** to tune against (actor-behaviour.md §8 items 7 and 8): GetDetected for the
  player standing, walking, running and sneaking at set distances, lit and dark, and the
  game's values of the fSneak* settings (the masters may override the documented
  defaults; `Settings::read` takes them when present).
- Not modelled: perk entry point "Ignore Running During Detection", NPC sneaking, melee
  and explosion sounds, SetAllVisible, GetLineOfSight (still a stub), the sneak meter on
  the HUD, sun shade in the light measure.
