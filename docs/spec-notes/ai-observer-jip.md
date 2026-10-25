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
| header `pc_detection` | GetPCDetectionState | -1 menu or loading, 0 hidden, 1 caution, 2 detected, 3 danger |

Actors in the loaded area get all of these. Tracked (named) actors, loaded or not, get
`proc_level` and `stable_package`: the processing level matters most for actors out of
the loaded area. Whether GetCurrentStablePackage is safe on an actor with no process is
not documented [hypothesis: safe; a crash would show in the run report].

`proc_level`, `stable_package`, `interrupt_package`, `fleeing` and `ragdolled` split
timeline segments like the other state fields. The rest are recorded per sample.

**Not in the tier yet:**
- Functions that return arrays (GetCombatTargets, GetCombatAllies, GetDetectedActors,
  GetDetectingActors). The probe shape returns one `|`-joined record of scalars per
  actor, and an array needs a shape of its own.
- The hit getters (GetHitAttacker, GetHitHealthDamage, GetHitLimbDamage, ...). They
  answer only inside an OnHit handler [documented: their pages], so they belong in the
  event probe, not the samples.
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
- **With** and **base** (`goodsprings_ab` × 3 each, plugin version 9): running.
  Results go to `C:\fnv-private\capture\observe\jip_ab\` (private).
