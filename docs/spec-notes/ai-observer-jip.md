# AI observer: the opt-in JIP tier

A capture tool note, not engine code. It adds to [ai-observer](ai-observer.md) a second,
opt-in tier of probe fields read with JIP LN NVSE functions, for the state no base-game or
xNVSE function reports. It also sets the A/B check that must pass before that data can
be used, and the rules for using it.

## 1. Why

The observer's first rule (owner decision 2026-10-08) is base game + xNVSE only
([ai-observer](ai-observer.md) §1). That leaves state the observer cannot read
([ai-observer](ai-observer.md) §1, §9, §12), and Layer 6 builds that state from wiki
text or guesses instead:

- **Processing levels** (high / mid-high / mid-low / low): our levels and when actors
  move between them come from the JIP pages alone [documented: GECK wiki (JIP)
  GetActorsByProcessingLevel, GetActorProcessingLevel; design/actor-behaviour.md §5.9]. The wiki says mid-low is "unknown at this time" for how
  often it is processed.
- **The package under an interrupt package**: while combat or dialogue runs, the
  observer sees only the interrupt package, not which package the actor goes back to.
- **Detection values**: `GetDetected` and `GetDetectionLevel` give a yes/no and four
  levels, not how close to detection an actor was.
- **Fights**: who the game considers an actor's targets and allies. Today "joined" vs
  "started" and the two sides are worked out from a graph [hypothesis,
  ai-observer.md §9].

## 2. Owner decisions

- 2026-10-08: observer only, base game + xNVSE only, no JIP or other plugins.
- 2026-10-25: the owner asked for an opt-in JIP tier, with a vanilla vs JIP A/B check on
  `goodsprings` first. The base game + xNVSE campaigns stay the default and the source of
  every behaviour fact.

## 3. What the tier reads

A session (or a campaign's `defaults`) with `"jip": true` adds the fields below
(`tools/capture/observe.py`, `JIP_PART`, `JIP_TRACKED_PART`, `JIP_HEADER`). Without the
key no JIP function is called, so all existing campaigns are unchanged. `observe.py run`
refuses a campaign with JIP sessions when there is no `jip*.dll` in `Data\NVSE\Plugins`
[observed 2026-10-25: the plugin's file is `jip_nvse.dll`].

Every meaning below is [documented: GECK wiki export, the page of each function, origin
JIP].

| Field | Function | Meaning |
|---|---|---|
| `proc_level` | GetActorProcessingLevel | 0 high (every frame), 1 mid-high (about every 18 s), 2 mid-low (unknown), 3 low (about every 120 s); -1 when the actor has no process, e.g. disabled |
| `stable_package` | GetCurrentStablePackage | the package in use, ignoring interrupt packages (combat, dialogue, ...) |
| `interrupt_package` | GetInterruptPackage | the interrupt package in use |
| `detection_value` | GetDetectionValue player | below 0: the actor does not detect the player; above 0: it does; the larger, the clearer. Stops updating once the caller is dead |
| `fleeing` | IsFleeing | fleeing combat, or running a Flee package |
| `ragdolled` | GetIsRagdolled | in ragdoll (knocked or pushed over) |
| `in_air` | IsInAir | not on solid ground |
| `move_flags` | GetActorMovementFlags | bitmask: 1 forward, 2 backward, 4 left, 8 right, 16/32 turn left/right, 256 walking, 512 running, 1024 sneaking, 2048 swimming, 4096 jumping, 8192 flying, 16384 falling, 32768 sliding |
| `velocity` | GetActorVelocity | total velocity (no axis given) |
| `combat_with_player` | IsInCombatWith player | in combat with the player |
| `unreachable` | GetTargetUnreachable | 1 when the actor is stuck, unable to find a path to its package's target |
| `combat_targets_*` | GetCombatTargets | for an actor in combat, the actors the game considers its potential targets at that moment |
| `combat_allies_*` | GetCombatAllies | for an actor in combat, the actors the game considers its allies at that moment |
| `detected_*` | GetDetectedActors | the actors it detects |
| `detecting_*` | GetDetectingActors | the actors detecting it |
| header `pc_detection` | GetPCDetectionState | -1 menu or loading, 0 hidden, 1 caution, 2 detected, 3 danger |
| header `player_light` | player.GetActorLightAmount | the light falling on the player, 0.0 complete darkness to 100.0 very bright |

An array field is recorded as its size, `<name>_n` (Ar_Size, -1 when the array was not
set [documented: GECK wiki Ar_Size]), and its first four elements, `<name>_1` to
`<name>_4`. In the game, an actor not in combat reads -1 for its targets and allies
[observed 2026-10-25, jip_check]. Each pair of arrays is its own probe part: four arrays
in one part made a return expression of about 560 characters, and the compiler refused
it with "Max script expression length inside parenthesis (512 characters)" [observed
2026-10-25, jip_check]. The job builder now refuses any part over 512.

**Hits.** In a JIP session that listens to OnHit or OnHitWith, those events' probe also
reads, from the target [documented: the JIP pages; valid "when called from ... the
corresponding NVSE event handlers"]:
- `hit_attacker` (GetHitAttacker), `hit_health`, `hit_limb` and `hit_fatigue` (the damage
  of this hit), `hit_weapon` (the base weapon), `hit_projectile` (the projectile or
  explosion reference; the page says one hit can come as up to three OnHit events);
- `hit_location` (GetHitLocation, GECK1; "available in a UDF event handler script for
  OnHit");
- `hit_flags`, a bitmask of GetHitExtendedFlag 0-8: 1 target blocking, 2 critical,
  4 fatal, 8 limb dismembered, 16 limb exploded, 32 limb crippled, 64 target's weapon
  broke, 128 sneak attack, 256 explosion hit.

In the gunfight of `jip_check` these answered for every hit [observed 2026-10-25]. Hits on
non-actors read -1, and the player under god mode reads 0 damage. Flag values seen
include 2, 4, 32, 38 and 46.

The timeline keeps the fields that change too often to start segments
(`detection_value`, movement, velocity, the arrays) as a per-actor `series` of [game
time, session, {field: value}] per sample.

Actors in the loaded area get all of these. Tracked (named) actors, loaded or not, get
`proc_level` and `stable_package`: the processing level matters most for actors out of
the loaded area. Whether GetCurrentStablePackage is safe on an actor with no process is
not documented [hypothesis: safe; a crash would show in the run report].

`proc_level`, `stable_package`, `interrupt_package`, `fleeing` and `ragdolled` split
timeline segments like the other state fields. The rest are recorded per sample.

**Not in the tier yet:**
- GetBufferedCells / GetCellBuffered (which cells the game keeps buffered): an array in
  the header, which the header scripts do not support yet.
- JIP's SetOnDialogTopicEventHandler (a script handler run when a topic or info is
  said): it would name the topic of each combat line, but the plugin only registers
  native handlers for xNVSE's events, so it needs a handler script compiled at run time.
- ClickMenuButton (choosing menu entries: dialogue, sleep/wait, cashiers). It makes the
  bot act, not just observe, and needs its own owner decision.
- JohnnyGuitar's SetOnProcessLevelChangeEventHandler. That is a second plugin, and it
  would need its own A/B check.

## 4. The A/B check (before any JIP-tier data is used)

The risk: JIP LN NVSE does more than add functions. It may also patch engine behaviour
[hypothesis: not checked against our install or its settings file]. If it changes how
AI runs, its data would describe a different game than the one we rebuild.

**Method.** The check uses `goodsprings_ab`, a short campaign of about 6 game hours
(about 15 real minutes a run): the town from 20:00 to 23:00, across the hour when
several residents' sleep packages begin, the saloon from 18:00 to 20:00, and an hour
beside Sunny. It has no `jip` key. A short campaign lets each side be run three times,
and three runs estimate run-to-run noise better than two runs of the 32-hour
`goodsprings` campaign would.
1. **With:** `goodsprings_ab` run three times (`observe.py run ... --repeat 3 --timeline
   ...`) from the `nvftest` save, with JIP LN NVSE and JohnnyGuitar NVSE installed at
   their default settings and never called. The run report's `nvse_plugins` records
   each DLL's name, size and SHA-256. Both plugins are checked together; if they move
   the AI, a second round with JIP alone tells which plugin does it.
2. **Base:** the same three runs with `Data\NVSE\Plugins` holding only
   `nvf_capture.dll`. The two sides can be run in either order.
3. Compare with `tools/capture/observe_compare.py --base <base timelines> --with <with
   timelines>`. For each session and actor seen in every run it compares: the share of
   time on each package and procedure, the number of segments, the distance walked and
   the cells visited. A `--with` value is listed as outside when it falls beyond the
   base runs' range by more than 0.05 of the session (shares) or 25 % (segments,
   distance). These are report thresholds [design] that sort what to look at first, not
   pass marks. Every row listed outside is explained in §5: run-to-run noise (two base
   runs are a thin sample), or a JIP effect.
4. Then `goodsprings_jip` (the full `goodsprings` sessions with `"jip": true`) once with
   JIP installed. This checks that the probes compile and answer. Its per-sample cost (`us` in the samples)
   against the base runs shows whether the extra probes change timing.

**Using the data.**
- If the A/B shows no JIP effect, JIP-tier fields can back facts tagged `[observed: JIP
  tier, campaign, date]`.
- If it shows an effect, JIP-tier data backs only state facts (which processing level,
  which stable package), never behaviour (timings, paths, package choice), and the
  effect is written up here.
- Either way, behaviour facts keep coming from base game + xNVSE runs. No goldens are
  made from JIP-tier runs.

**Install (owner).** Our tools never write into the game folder. The owner copies the
plugins' DLLs from their official releases into `Data\NVSE\Plugins`, and takes them out
again for the base half and afterwards, so the default campaigns run on plain xNVSE.

**Clean room.** The JIP functions are used only as readers. Their meanings come from the
wiki pages cited in §3, not from JIP's source code, and nothing of JIP ships.

## 5. Results

- 2026-10-25: a first base run of the full `goodsprings` campaign was stopped after
  three of its four sessions, in favour of the short campaign above. It is not used.
- **The with set-up, 2026-10-25** [observed: `Data\NVSE\Plugins` on the owner's install]:
  `jip_nvse.dll` (502,272 bytes, SHA-256 9d277964…), `johnnyguitar.dll` (286,720 bytes,
  SHA-256 456b6ce7…), `nvf_capture.dll` (version 9), and `JohnnyGuitar.ini` at its
  shipped settings. JIP has no settings file. The archive also brought
  `textinput\texteditmenu.xml`, a menu layout with no code. The ini's own comments
  describe what it switches. On: `bFixItemStackCount` (dropping more than 32,000 items),
  `bJIPFixes` ("various fixes" to JIP 57.30) and, under `[JIP]`,
  `bFixPositiveChemDuration` (the Modify Positive Chem Duration perk entry point limited
  to chems). Off: `bFixFleeing` (NPCs fleeing in CAUTION with no enemies),
  `bFixNPCShootingAngle`, `bFixDeathVoicelines` and the rest. So the with set-up is not
  the original game, though none of the switches that are on is about AI.
- **The A/B check, 2026-10-25** (`goodsprings_ab` × 3 with JIP + JohnnyGuitar, then × 3
  with `nvf_capture.dll` only; plugin version 9; private results in
  `C:\fnv-private\capture\observe\jip_ab\`, `with_vs_base.json`). All 18 sessions ended
  at their game time; none was cut off and no timeline has problems. 45 (session,
  actor) pairs were seen in every run.
  - **The same on both sides** [observed]: every NPC's package and procedure shares
    (schedules, sandboxing, bedtime at 22:00, the saloon evening, Sunny's dialogue
    package and target) are within the base runs' range. The cost of a sample is the
    same (median about 2.4 ms both sides).
  - **Different in all three runs** [observed]:
    - The animals' brief GET_UP and TRAVEL spells inside WANDER (each one sample, about a
      second, in `town_bedtime`). GET_UP spells per run: 8, 7, 8 without the plugins;
      6, 3, 2 with them, mostly the bighorners (8, 8, 10 against 6, 3, 2). The ravens'
      TRAVEL spells: 2, 1, 2 without; 0 with.
    - GSVictorRef, a tracked actor not loaded at that moment: his first sample in
      `saloon_evening` is in cell 0x000DAEB8 with the plugins and 0x000DAEBA without.
      The rest of his patrol matches.
    - 359 samples per `town_bedtime` session with the plugins against 363 without, every
      run: about 1 % fewer game loops in the same three game hours.
    - Packages made at run time are numbered differently (Sunny's FF001323 against
      FF001313; same type, procedure and target). `observe_compare.py` now compares
      such packages by type.
  - **Cause:** not known. Candidates [hypothesis]: the slightly lower frame rate; a fix
    in JIP or JohnnyGuitar (`bJIPFixes` is on); off-screen processing between the save
    loading and the clock moving. A round with JIP alone would tell the two plugins
    apart.
  - **Verdict:** the plugins change something, though not the NPC schedules and packages
    measured here. So the §4 rule applies: JIP-tier data backs **state facts only**
    (processing level, stable and interrupt package, detection value), tagged
    `[observed: JIP tier ...]`. Behaviour facts and goldens keep coming from runs with
    `nvf_capture.dll` alone, and the default campaigns are run with the other plugins'
    DLLs taken out.
