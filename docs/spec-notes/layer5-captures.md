# Layer 5 in-game captures: quest state and quest-script behaviour (workstream 5C)

Captures made with the `nvf_capture` xNVSE plugin (version 7) on the owner's install (FNV
1.4.0.525, Steam, xNVSE 6.4.9, the ten masters), unattended through
`tools/capture/capture_run.py`. They establish the quest state of the two capture saves
and the behaviour that docs/design/layer5.md §3.3–3.5 left as [hypothesis]. The tool is
`tools/capture/capture_quests.py` (README there); scenario inputs are in
`parity-lab/inputs/layer5/`, scenarios in `parity-lab/scenarios/layer5_quests.json`.

Private files (never committed): definitions, jobs and outputs under
`C:\fnv-private\capture\` (`quest\`, `out\`), goldens under `C:\fnv-private\goldens\quest\`.
Nothing was saved in game; every change a job makes is lost when the game quits.

Tags as everywhere: [documented] (source named), [observed] (this capture or the masters),
[hypothesis].

Summary of what was settled (details and evidence in §2 and §5):

| Question (layer5.md) | Answer | Tag |
|---|---|---|
| Does `SetStage` start a stopped quest? | yes | [observed] |
| `GetStage`: highest or last stage set? | the highest | [observed] |
| Does a lower stage's result script run? | yes, and its stage is marked done | [observed] |
| Same stage again, no "allow repeated stages" | the result script does not run again | [observed] |
| Same stage again, with the flag | the result script runs again | [observed] |
| Undefined stage | no effect | [observed] |
| When does the result script run? | inside `SetStage`, before it returns | [observed] |
| Complete / Fail Quest stage flags | set "completed" (also for Fail); neither stops the quest | [observed] |
| Quest script delay | the `DATA` delay whenever it is non-zero, whatever the "Default Script Processing Delay" flag says; delay 0 (flag clear) → 5 s; a delay under one frame → every frame | [observed] |
| short / long / int storage | no 16-bit wrap; truncation toward zero; out of 32-bit range → −2147483648 | [observed] |
| float storage | double precision | [observed] |
| Whole-number literals | saturate at 2147483647; a negative literal is the negation of that | [observed] |

## 1. How jobs name quests [observed, trials `q5_trial`, `q5_trial2`, 2026-10-09]

Jobs: `tools/capture/jobs/q5_trial.steps.json` and `q5_trial2.steps.json`
(`make_quest_jobs.py`), save `nvftest`.

- **Quest Editor IDs do not compile in job scripts.** `GetStage VCG01`,
  `GetQuestRunning VCG01`, `VCG01.fTimer` (read with `let` or `set`) all failed with
  "script did not compile". Prediction before the run was the opposite (quests keep their
  Editor IDs at run time like globals); refuted. Globals still compile (`ScroungerOdds`
  read 100).
- **Console lines with a quest Editor ID have no effect** in these unattended sessions:
  `set VMS22.VMS22DinoPic to 5` left the variable 0, `StartQuest VMQ03aTimer` left the quest
  stopped, although `RunScriptLine` reported success. Why is not known [hypothesis: the
  console resolves Editor IDs only for some form types at run time].
- **A quest in a ref variable works**: `let rQ := BuildRef <FalloutNV.esm index> <local FormID>`,
  then `GetStage rQ`, `GetStageDone rQ n`, `GetQuestRunning rQ`, `GetQuestCompleted rQ`,
  `GetObjectiveDisplayed rQ n`, `GetObjectiveCompleted rQ n`, `SetStage rQ n`,
  `StartQuest rQ`, `StopQuest rQ` all compile and act on the quest.
- **Quest variables by name through xNVSE**: `GetVariable "name" rQ` reads them and
  `SetVariable "name" value rQ` writes them. `GetVariable` of a name the quest script does not
  declare returns 0 without an error.
- `$` of a float gives six decimals (`0.333333`), so numbers are always returned as numbers.

So `capture_quests.py` builds every quest with `BuildRef` and reads and writes quest
variables with `GetVariable` / `SetVariable`. The vanilla `set quest.var to …` statement
cannot be issued from a job; what a Set statement stores was measured on the job script's
own locals instead (§5.5).

## 2. Quest state of the capture saves [observed, `q5_state_nvftest`, `q5_state_nvfdoc`]

Definitions from `capture_quests.py defs` (fnvtool esm decode of FalloutNV.esm, 18 quests:
the intro chain VCG00–VCG04, VCG01Test, VMQ01, VMQTops, Goodsprings quests VMS16, VMS16b,
VFreeformGoodsprings, VDialogueGoodspringsGeneric, VERGoodsprings01, vGSRadioQuest,
CGTutorial, and Generic, VMS22, VMQ03aTimer used by §5), 584 reads per save. State
documents: `goldens/quest/state.nvftest.json`, `state.nvfdoc.json` (private,
`fnv.quest.state/1`).

**Both saves are at the same point of the chain**: the only differences are VCG04's
player-position variables and VERGoodsprings01's random-encounter frame counter and roll.

| Quest | Running | Completed | GetStage | Stages done (of those defined) | Objectives | Variables (non-zero) |
|---|---|---|---|---|---|---|
| VCG00 | no | no | 100 | 0, 90, 95, 100 (not 5–85) | — | bRunTimer 1, fTimer −0.0140 |
| VCG01 | no | yes | 200 | all but 9, 17, 20, 37, 78 | 10, 30, 40, 50 displayed and completed | bRunTimer 1, fTimer −0.0160, bBeganTest 1, nDocReaction 4, nMostExtremeStat 7, fMaxDeviation 4.5, fCurrentValue −0.5, fAbsCurrentValue 0.5 (bChooseSex 0) |
| VCG01Test | no | no | 0 | — | — | the psych-exam scores (bDoFinalCalculation 2) |
| VCG02 | **no** | no | 5 | 5 | 3 displayed | — |
| VCG03 | no | no | 0 | none | none | — |
| VCG04 | yes | no | 0 | (no stages) | — | Initialized 1, borders set, Done 0 |
| VMQ01 | yes | no | 10 | 10 | 10, 25, 30 displayed | — |
| VMQTops, CGTutorial (stage 2 done), VFreeformGoodsprings (bVictorPatrol 1), VDialogueGoodspringsGeneric, VERGoodsprings01, vGSRadioQuest, Generic, VMS22 | yes | no | 0 (CGTutorial 2) | — | — | — |
| VMS16, VMS16b, VMQ03aTimer | no | no | 0 | none | none | — |

Reading [observed unless tagged]: VCG01 ("Ain't That a Kick in the Head") is complete; its
stage 200 started VMQ01 ("They Went That-a-Way", stage 10, objectives 10/25/30 shown) and
VCG04 (character revision, waiting for the player to leave the Goodsprings area). VCG02
("Back in the Saddle") has stage 5 done and objective 3 ("talk to Sunny") shown, but is
**not running**, although SetStage starts a stopped quest (§5.1); stage 5 is set by
VGenericTimer's countdown (`nEvent` 3 in the save) together with VCG01's 200 [observed in
the masters' source text]. Why VCG02 is stopped is open. VCG03 and the Goodsprings
side quests are untouched. VCG00's stages 5–85 are not done although its timers were
[hypothesis: the opening cinematic path sets only 0, 90, 95, 100 in this version]. Both
VCG00 and VCG01 stopped with `bRunTimer` 1 and `fTimer` a little below 0: the countdown
ran out and the stage switch then stopped the quest.

**Neither save is at the start of VCG01.** A save at the very beginning of a new game
(before Doc Mitchell speaks) is needed to capture the chain itself (owner, §7).

## 3. Predictions (written before each run)

Written in the session notes before the behaviour runs (2026-10-09, about 02:30, plan
"p3"; the delay follow-ups at 02:45 after the first delay results):

- P1 SetStage on a stopped quest starts it — **held**.
- P2 GetStage after 50 then 10 is 50 (highest) — **held**.
- P3 a lower stage's result script still runs — **held**.
- P4 without "allow repeated stages", setting a done stage again does not run its script — **held**.
- P5 with the flag it runs again — **held**.
- P6 an undefined stage changes nothing — **held**.
- P7 a Fail Quest stage leaves GetQuestCompleted 0 — **refuted** (1; the GECK wiki's
  GetQuestCompleted page says failed quests count as completed [documented]).
- P8 a Complete Quest stage with an empty script sets completed and leaves the quest running — **held**.
- P9 the result script has run when SetStage returns — **held** (also [documented: GECK
  wiki SetStage notes]).
- P10 VCG03 stage 10 sets its variable from the player's item count and enables its
  references — **held**.
- P11 a result script with if/else and AddItem (held-out scenario; result not given here).
- P12 integer variables truncate toward zero, no 16-bit wrap — **held**; the 32-bit edge
  (−2147483648 for out-of-range floats; saturating literals) was not predicted.
- P13 float variables hold doubles — **held**.
- P14 the "Default Script Processing Delay" flag gives the 5 s default — **refuted** (VCG01,
  flag set, DATA 0.01, runs every frame). Follow-up prediction (02:45): a non-zero DATA delay
  is used whatever the flag; DATA 0 gives 5 s — **held** for the visible scenarios.
- P15 a stopped quest's script does not run — **held**.
- P16 the first countdown drop after StartQuest is larger than a period — **mixed** (§5.4).

## 4. Contract for the candidate: `fnvtool script run` (workstream 5B)

```
fnvtool script run <data_dir> <plugin>... --schema <dir> --actions <file>   → fnv.script.trace/1 on stdout
```

This is docs/design/layer5.md §3.8 made concrete for the `L5.quest.*` scenarios. It
follows `fnv-sim`'s `sim run` (docs/spec-notes/simulation.md §7): the same load-order
arguments, the same naming of forms (global FormID `"0x%08X"`, Editor ID, or `"player"`),
the same item and reference operations, and observations keyed by `label`. The capture
side of the same file is `capture_quests.py job` / `golden`, so the inputs are run by the
original game and by the candidate unchanged.

### 4.1 Input `fnv.script.actions/1`

```json
{"format": "fnv.script.actions/1", "seed": 0, "frame_seconds": 0.016666666666666666,
 "title": "…", "save": "nvftest",
 "given": [{"kind": "quest", "quest": "VMS16", "running": false, "completed": false, "stage": 0,
            "done": [], "not_done": [5, 10], "objectives": [{"index": 5, "displayed": false}],
            "vars": {"nGangerDeathCount": 0}},
           {"kind": "global", "name": "ScroungerOdds", "value": 100},
           {"kind": "item_count", "of": "player", "item": "0x00136A1D", "count": 0},
           {"kind": "disabled", "of": "0x0015D982", "disabled": true}],
 "actions": [{"op": "set_stage", "quest": "VMS16", "stage": 50},
             {"op": "get_quest", "quest": "VMS16", "label": "VMS16 after stage 50",
              "fields": ["running", "stage", "done.50", "objective.10.displayed", "var.bMetJoeCobb"]}]}
```

- `frame_seconds`: the runner's fixed frame step (deterministic traces, layer5.md §3.4).
  The game ran at about 60 game loops per second [observed: 1,433 loops for 24 s,
  xnvse-capture.md §7].
- `save` is informational: which capture save the golden came from. The candidate cannot
  load saves; it starts from the new-game state of the load order and applies `given`.
- `given`: the facts of the save that the scenario depends on. The candidate **sets**
  them before action 0 (quest running/completed flags, current stage, stages done and not
  done, objective flags, quest variables by name, globals, item counts, disabled flags).
  They produce no observation. When the golden is built, every `given` is checked against
  what the save reported; a mismatch stops the golden (`capture_quests.py golden`), so a
  golden never exists for a precondition the save did not have.
- Quests are named by Editor ID; quest variables by name (the candidate maps the name to
  its index through the quest script's `SCVR`/`SLSD`, layer5.md §2).

Operations (all names as in the examples; a `label` makes an observation):

| op | fields | what it does | observation |
|---|---|---|---|
| `set_stage` | `quest`, `stage` | `SetStage quest stage` from a script | — |
| `set_stage_read` | `quest`, `stage`, `field`, `label` | `SetStage`, then reads one quest field **in the same script** | `{<field>: value}` |
| `start_quest`, `stop_quest` | `quest` | `StartQuest` / `StopQuest` | — |
| `set_quest_var` | `quest`, `var`, `value` | stores the number in the quest variable as it is (captured with xNVSE's `SetVariable`, §1; used only for values every variable type keeps exactly) | — |
| `set_local` | `type` (`short`, `long`, `int`, `float`), `value`, `label` | a Set statement `set x to <literal>` into a fresh local of that type, read back | `value` |
| `set_objective` | `quest`, `index`, `displayed` and/or `completed` (bool) | `SetObjectiveDisplayed` / `SetObjectiveCompleted` | — |
| `get_quest` | `quest`, `fields`, `label` | reads each field | one key per field |
| `sample_var` | `quest`, `var`, `frames`, optional `runs` and `skip`, `label` | reads the variable once per frame for `frames` frames | without `runs`: `changes` (int, how often the value changed); with `runs`: `drops`, `runs` changes after the first `skip`, each as previous − new value (the GetSecondsPassed countdowns of §5.4 make each drop the time between two runs of the quest script) |
| `wait_frames` | `frames` | lets frames pass | — |
| `add_item`, `remove_item`, `remove_all_items`, `enable`, `disable`, `set_global`, `activate`, `get_item_count`, `get_disabled`, `get_global` | as in `fnv.sim.actions/1` | as in `sim run` | `count` (int), `disabled` (bool), `value` (number) |

**Literals.** `set_local` writes `value` into the script as text the way the capture does
(`capture_quests.literal`): a whole number without a decimal point (`3000000000`), any
other number as its shortest round-trip decimal, never in exponent form (`0.00000001`).
The compiled form keeps numbers as text (layer5.md §2), so the candidate evaluates that
text the way the game's expression evaluator does; §5.5 shows that this matters
(whole-number literals saturate).

Quest fields: `running`, `completed` (GetQuestRunning, GetQuestCompleted; bool), `stage`
(GetStage; int), `done.<n>` (GetStageDone; bool), `objective.<n>.displayed`,
`objective.<n>.completed` (GetObjectiveDisplayed / GetObjectiveCompleted; bool),
`var.<name>` (the quest variable; number as stored).

**Frames between actions.** In the game every step takes at least one game loop, and the
capture pauses after changes; the quest scripts run in between. The candidate advances the
same number of frames so quest scripts get the same chances to run: `set_stage` and
`set_stage_read` 1 + 30; `start_quest`, `stop_quest`, `set_quest_var`, `set_objective` 1 + 10;
`set_local` 1;
the `fnv.sim.actions/1` operations as `capture_steps.py` schedules them (1 + 10, or 1 + 30
for `activate`, `remove_all_items`); `get_quest` 1 per field; `sample_var` 1 per frame;
`wait_frames` its `frames`. Scenarios whose observations depend on this count are
avoided (the timing scenarios compare `drops`, which do not).

### 4.2 Output `fnv.script.trace/1`

```json
{"format": "fnv.script.trace/1", "seed": 0, "frame_seconds": 0.016666666666666666,
 "observations": [{"label": "VMS16 after stage 50", "step": 2, "running": true, "stage": 50,
                   "done.50": true, "objective.10.displayed": false, "var.bMetJoeCobb": 0},
                  {"label": "VMQ03aTimer timer while running", "step": 5, "drops": [0.5, 0.5, 0.5]}],
 "events": [{"frame": 31, "kind": "stage_set", "quest": "0x00104EAE", "stage": 50},
            {"frame": 31, "kind": "objective_displayed", "quest": "0x00104EAE", "index": 50}],
 "final": {"quests": {"0x00104EAE": {"running": true, "stage": 50, "stages_done": [50]}}}}
```

- `observations`: one per labelled action, in action order, with `label`, `step` (the
  action index) and the fields above. A field that cannot be read is left out and the
  observation gets `"errors": {"<field>": "<why>"}`. The golden has the same shape, plus
  `diagnostics` (per `sample_var`: loops between changes, all drops; not compared) and
  `mod_indices`.
- `events` and `final` are the candidate's diagnostics (layer5.md §3.8: stage set,
  variable written, quest started/stopped/completed/failed, objective shown/completed,
  reference enabled/disabled, item added/removed, message shown, unhandled call, each
  with its frame). They are not compared by the `L5.quest.*` scenarios: the game shows
  only the states the jobs query, not an event log.

Lab comparator: **keyed** on `label` over `/observations` (every field the golden row
has; extra candidate fields are ignored); the timing scenarios compare with `abs_tol`
0.05 (seconds), the others exactly.

## 5. Behaviour [observed unless tagged; save `nvftest` unless named; each capture repeated once, see §6]

### 5.1 SetStage, GetStage, GetStageDone (`L5.quest.stage_order`, `.repeated_stage`, `.result_effects`)

VMS16 ("Ghost Town Gunfight"; its result scripts only change objectives), stopped, no
stage done:

| Step | running | GetStage | done | objectives (D displayed, C completed) |
|---|---|---|---|---|
| start | no | 0 | none | none |
| SetStage 50 | **yes** | 50 | 50 | 10 C (never displayed), 50 D |
| SetStage 10 (lower) | yes | **50** | 10, 50 | 5 C, 10 D (C cleared), 50 D |
| objective 10 hidden by script, SetStage 10 again | yes | 50 | 10, 50 | 10 stays hidden: **script not run** |
| SetStage 15 (no such stage) | yes | 50 | 15 not done | unchanged |
| SetStage 5 (lower) | yes | 50 | 5, 10, 50 | 5 D (its C cleared) |
| SetStage 110 (Fail Quest flag; script `StopQuest`) | no | **110** | 110 too | unchanged; GetQuestCompleted **1** |

- SetStage starts a stopped quest; GetStage is the highest stage done; a lower stage is
  still marked done and runs its result script; a done stage set again does not run its
  script when the quest lacks "allow repeated stages"; an undefined stage does nothing.
  These agree with the GECK wiki's SetStage and GetStage pages [documented].
- Generic ("allow repeated stages"): stage 70's script sets the global `ScroungerOdds` to
  25; reset to 0 and set again, it is 25 again: **the script runs every time**. GetStage
  stays 70 after the lower stage 60 (whose script ran: FortuneFinderOdds 90). GetStageDone
  is 1 for each.
- VCG03 stage 10 (no repeat flag): the result script's
  `Set VCG03.nHealingPowdersAtStart to Player.GetItemCount NVHealingPowder` gave 2 with two
  powders added, and the value was already there when read **in the job script that
  called SetStage, on the line after it**: the result script runs inside SetStage. The
  Xander root, broc flower and the first survival critter (disabled in the save) were
  enabled. After adding a powder and disabling the root, SetStage 10 again left the
  variable at 2 and the root disabled.

### 5.2 Objectives [observed, VMS16]

- `SetObjectiveCompleted q n 1` on an objective never displayed marks it completed without
  displaying it.
- `SetObjectiveDisplayed q n 1` **clears the completed flag** (objective 10, then 5: C → not C).

### 5.3 Complete and Fail Quest stage flags (`L5.quest.complete_flag`, `.stage_order`)

VMS22 (running, "allow repeated stages"): stage 90 (Complete Quest, empty script) →
completed 1, still running, GetStage 90. Then stage 100 (Fail Quest, empty script) →
completed stays 1, still running, GetStage 100. VMS16's Fail Quest stage 110, whose script
calls StopQuest: running 0, completed 1. So the flags set the completed state (failed
counts as completed, [documented: GECK wiki GetQuestCompleted]); only the script's
StopQuest stops a quest.

### 5.4 Quest script delay and GetSecondsPassed (`L5.quest.delay_*`)

Method: quest scripts that count a timer down by `GetSecondsPassed` in their GameMode
block. The job sets the timer to 100000 (and the script's run flag to 1), samples it every
game loop, starts the quest if it was stopped, and keeps the drop at each change: the
time between two runs of the quest script.

| Quest | flag "Default Script Processing Delay" | DATA delay | runs every | drops (s) |
|---|---|---|---|---|
| VMQ03aTimer | clear | 0.5 | 30 loops | 0.5 (0.4921875–0.5078125) |
| CGTutorial (running) | set | 1.0 | 60 loops | 1.0 |
| VCG01 | set | 0.01 | every loop | 0.015625 (sometimes 0.03125) |
| VCG00 | clear | 0.01 | every loop | 0.015625 |
| VMS40Timer | clear | 0 | 300 loops | 5.0 (4.984375–5.015625) |
| (held out) | | | | |

- **The DATA delay is used whenever it is non-zero, with or without the flag; 0 (flag clear) means 5 s**
  (the GECK wiki's SetQuestDelay page gives 5 s as the INI default, and 0 as "use the
  default" [documented]). A delay shorter than a frame runs the script every frame. What
  the flag does is not visible here [hypothesis: an editor-side default for the field].
- A stopped quest's GameMode does not run (no change in 120 loops, every case).
- **GetSecondsPassed comes in steps of 1/64 s** (0.015625): every drop is a multiple of it,
  while the game ran at about 60 loops per second (300 loops per 5 s)
  [hypothesis: the game's frame timer has the 15.625 ms resolution of the Windows system
  clock]. 0.5 s came as 32/64, sometimes 31 or 33.
- The first run after StartQuest is not regular: VMQ03aTimer, VCG00 and VCG01 ran within
  the 12 loops between StartQuest and the first sample (drops of 1/64), VMS40Timer only
  about 3.9 s after StartQuest, with a drop of 3.95 s [hypothesis: the time to a quest's
  first run depends on a delay counter that runs while the quest is stopped]. The
  scenarios therefore skip the first change seen (`skip` 1).

### 5.5 What a Set statement stores (`L5.quest.var_storage`)

`set xV to <literal>` with `xV` a local of the job script declared `short`, `long`,
`int` or `float`, read back in the same script. The compiled form records only Integer vs
FloatOrRef for a variable (`SLSD`), so a quest script's locals are expected to behave the
same [hypothesis].

| Literal | short / long / int | float |
|---|---|---|
| 32768, 40000, 65536, 70000, −32769, −40000 | unchanged (no 16-bit wrap) | — |
| 2.7, 2.5, 0.5, −0.5, −2.5, −2.7, 0.3333… | 2, 2, 0, 0, −2, −2, 0 (toward zero) | unchanged |
| 40000.5, 2147483647.5 | 40000, 2147483647 | — |
| 2147483647, 2147483648, 3000000000 (no decimal point) | 2147483647, **2147483647, 2147483647** | 3000000000 → **2147483647** |
| −3000000000 | **−2147483647** | **−2147483647** |
| 3000000000.5, −3000000000.5 | **−2147483648**, −2147483648 | unchanged |
| 0.3333333333333333, 0.1, 16777217, 0.00000001, 123456789.123, 4294967296.5 | — | unchanged (double precision) |

- Integer variables: truncation toward zero; out of the 32-bit range the result is
  −2147483648 [hypothesis: the x86 truncating conversion's "integer indefinite" value].
- **Whole-number literals saturate at 2147483647 even into a float**, and a minus sign
  negates the saturated value (−2147483647) [hypothesis: the literal is read as a 32-bit
  integer and negated as a separate step]; literals with a decimal point are doubles.
- Float variables keep doubles. xNVSE's `SetVariable`/`GetVariable` on quest variables
  also returned the doubles written (`fTimer` 100000, VMS22's variables).

### 5.6 Result script with if/else and AddItem (`L5.quest.result_additem`, save `nvfdoc`)

Held out; its result is in the private golden only.

## 6. Scenarios and goldens

`parity-lab/scenarios/layer5_quests.json`, comparator keyed on `label` over
`/observations`; inputs `parity-lab/inputs/layer5/quest_<name>.actions.json` (written by
`make_inputs.py` there); goldens `goldens/quest/quest_<name>.trace.json` (private). The
candidate command does not exist yet, so all twelve **error** in the lab until 5B lands
(checked: `python -m fnvlab run --layer 5` → 0/12 pass, 12 error).

| Scenario | Rows | Tolerance | Notes |
|---|---|---|---|
| `L5.quest.stage_order` | 8 | exact | §5.1–5.3 |
| `L5.quest.repeated_stage` | 7 | exact | §5.1 |
| `L5.quest.result_effects` | 7 | exact | §5.1 |
| `L5.quest.complete_flag` | 2 | exact | §5.3 |
| `L5.quest.var_storage` | 46 | exact | §5.5 |
| `L5.quest.delay_data`, `.delay_flagged_short`, `.delay_frame`, `.delay_flagged_1s`, `.delay_zero` | 4, 4, 4, 2, 4 | 0.05 s | §5.4 |
| `L5.quest.result_additem` | 3 | exact | **holdout** (save nvfdoc) |
| `L5.quest.delay_flagged_zero` | 4 | 0.05 s | **holdout** |

Stability: every job was run twice (plans p3/p5 and p6, separate game sessions). The second
run compared with the golden under each scenario's comparator scored 1.0 everywhere; the
delay drops differed by up to 1/32 s between runs (inside the 0.05 tolerance), everything
else was identical. Every `given` precondition matched the save in both runs.

Rebuilding (owner's machine; take the run lock first, never save in game):

```
python parity-lab/inputs/layer5/make_inputs.py
python tools/capture/capture_quests.py job parity-lab/inputs/layer5/quest_<name>.actions.json --name q5_quest_<name> --start <save> --defs C:/fnv-private/capture/quest/defs.json --data "<Data>" -o C:/fnv-private/capture/quest/q5_quest_<name>.steps.json
python tools/capture/capture_run.py --game "<FNV>" --save <save> --kill-on-timeout C:/fnv-private/capture/quest/q5_quest_<name>.steps.json
python tools/capture/capture_quests.py golden C:/fnv-private/capture/out/q5_quest_<name>.<time>.jsonl --actions parity-lab/inputs/layer5/quest_<name>.actions.json -o <somewhere outside goldens>/quest_<name>.trace.json
```

One scenario per game session (the jobs change quest state). `golden` refuses to write when
a `given` does not match the save.

## 7. Open items and owner review

- **A save at the start of a new game** (before Doc Mitchell speaks, VCG01 stage 0 or 1) is
  needed to capture the intro chain itself (layer5.md §4 item 4). It can only come from the
  owner: start a new game, and as soon as the player can look around, open the console and
  `save nvfnew` (the plain name lets `capture_run.py --save nvfnew` load it). Captures from
  it would drive VCG01's stages with SetStage and record stage, objectives, variables and
  the player's controls after each.
- VCG02 has stage 5 done but is not running in both saves (§2), against §5.1's "SetStage
  starts a stopped quest". Not explained.
- Job scripts cannot issue the vanilla `set quest.var to …` (§1); §5.5 rests on the job's
  own locals and the hypothesis that a quest script's locals store the same way.
- The first run after StartQuest (§5.4) is not modelled; the scenarios skip it.
- Not captured: the order of scripts within one frame, whether GameMode runs on the frame
  of an activation, object scripts (OnActivate, OnLoad), conditions (`CTDA`) on stage
  entries (none of the stages used has any). They need further jobs.

## 8. Round 2: stage entries, conditions, object scripts, timing (2026-10-09)

Second capture round (workstream 5C, branch `claude/layer5-captures-2`), made without
reading the runtime (`crates/fnv-game`, `crates/fnv-script/src/vm*`) or scripts.md §4 and
later, from the original game's behaviour only. Same install, plugin (v7) and saves as
§1–6; unattended through `capture_run.py`, one job per game session, nothing saved.
**No save `nvfnew` exists** (the Saves folder holds `nvftest`, `nvfdoc`, the owner's own
numbered saves and an autosave), so the intro quest VCG01 could not be captured (§7).

Diagnostic jobs: `tools/capture/jobs/q5b_trial_test{,2,3}.steps.json` (save `nvftest`)
and `q5b_trial_doc{,2,3}.steps.json` (save `nvfdoc`), written by
`tools/capture/jobs/make_quest2_jobs.py`. Scenario inputs:
`parity-lab/inputs/layer5/quest2_*.actions.json` (`make_inputs2.py`), scenarios
`parity-lab/scenarios/layer5_quests2.json`, goldens `goldens/quest2/` (private).
Candidates were found by scanning the masters' records and script sources (read-only).

### 8.1 Summary

| Question | Answer | Tag |
|---|---|---|
| (a) A stage with several log entries whose conditions all pass | **every entry's result script runs**, in entry order | [observed] |
| (b) AND/OR grouping in a condition list | **OR binds tighter than AND**: a run of conditions joined by OR flags is one group, the groups are ANDed; not left to right, not AND first | [observed] |
| (c) Stray ElseIf after its If chain was closed by EndIf | **its condition is tested and its body runs when true**; the stray EndIf after it does nothing visible | [observed] |
| (d) GameMode on the frame of an activation | runs (no run skipped or doubled); OnActivate itself runs **inside** the Activate call | [observed] |
| (d) OnAdd / OnLoad against GameMode | an item's GameMode acts on its OnAdd by the next frame; OnLoad after Enable runs by the next frame, not inside Enable | [observed] |
| (d) Quest scripts against object scripts within one frame | not settled (§8.3 d) | — |
| (e) GetSecondsPassed after a long menu or a loading screen | **the gap is not counted**: the first value after it is one ordinary frame (or the quest's own delay) | [observed] |
| (f) Object-script variables of a reference | xNVSE's `ref.GetVariable "name"` / `ref.SetVariable "name" value` read and write them | [observed] |
| (f) OnActivate of a scripted `Activate` | runs only with Activate's second parameter (run-OnActivate flag) set; without it the default activation happens and the block does not run, also on dead actors | [observed] |

### 8.2 Contract additions (`fnv.script.actions/1`, §4)

Additions only; every existing operation, field and observation is unchanged. Frames
are counted as in §4.1 unless given here.

| op | fields | what it does | observation |
|---|---|---|---|
| `activate` (existing) + `run_on_activate: true` | as in `fnv.sim.actions/1` | `Activate <actor> 1`: the target's OnActivate block runs instead of the default activation ([documented: GECK wiki Activate, RunOnActivateBlockFlag]); 1 + 30 frames | with `label`: `opened` (container menu open, as before) |
| `set_ref_var` | `ref`, `var`, `value` | sets a variable of the reference's object script (xNVSE `ref.SetVariable`); 1 + 10 | — |
| `get_ref` | `ref`, `fields`, `label` | reads `var.<name>` (object-script variable), `dead` (GetDead), `disabled`; 1 per field | one key per field (`dead`, `disabled` bool) |
| `activate_read` | `target`, `actor` (default `player`), optional `run_on_activate`, `read` = `{"ref", "field"}` (a `get_ref` field) or `{"quest", "field"}` (a `get_quest` field), `label` | Activate and the read **in the same script**; 1 + 30 | `{<field>: value}` |
| `sample_ref_var` | `ref`, `var`, `frames`, optional `runs`, `skip`, `values`, `label` | as `sample_var`, for an object-script variable | as `sample_var` |
| `sample_var` (existing) + `values: true` | | | adds `values`: the successive distinct values seen |
| `add_item_read` | `to`, `item`, `count`, `read` (items), `label` | AddItem (silent) and the counts of the `read` items held by `to`, in the same script; 1 frame, no pause | `count.<item>` (int) per item |
| `sample_items` | `of`, `items`, `frames`, `label` | the counts once per frame | per item: `first.<item>` (first sample), `final.<item>` (last), `settled.<item>` (index of the first sample from which the count stays final) |
| `menu_gap` | `via` (a container), `frames`, `quest`, `var`, `after`, `label` | the player activates `via` (its container menu opens); the menu stays open `frames` frames counting the opening frame; the quest variable is read in its last frame; the menu closes (console `CloseAllMenus`) and the variable is read once per frame for `after` frames | `drop`: the value read in the menu minus the value at the first change after it |
| `kill` | `target` | `Kill` (no killer); 1 + 60 | — |
| `evaluate_package` | `of` | `EvaluatePackage`; 1 + 60 | — |
| `is_current_package` | `of`, `package`, `label` | whether the actor's current package (xNVSE `GetCurrentPackage`) is `package`; 1 | `current` (bool) |

`given` gains the kind **`ref`**: `{"kind": "ref", "ref": <FormID>, "vars": {...},
"dead": bool, "disabled": bool}` (any subset), checked like the others.

### 8.3 Findings [observed unless tagged]

**(a) Several log entries in one stage.** ControlRumble (FalloutNV 0x00070EC9, allow
repeated stages) has five entries in stage 0, none with conditions; entry 0 resets
`Stage1Fired`…, entry 1 `Stage1FireOnTimerValue`…, entry 2 `stage1RumbleLeft`…, entry 3
`Stage1Duration`… and sets `timerMaxValue` to 10, entry 4 calls `StopQuest ControlRumble`.
With the five variables set to 7, `SetStage ControlRumble 0` left all of them 0,
`timerMaxValue` 10 and the quest stopped (`q5b_trial_test`; scenario
`L5.quest.multi_entry_stage`, both runs). So every entry whose conditions pass runs its
result script, in record order (entry 4's StopQuest came last: the quest ended stopped
although SetStage starts a stopped quest, §5.1). Both saves already show that end state
(stopped, stage 0 done, `timerMaxValue` 10), consistent with the quest script's own
`SetStage ControlRumble 0` at game start. No stage in the masters has two entries with
conditions that can both pass and observable, harmless effects, so an entry whose
conditions fail is not captured [hypothesis: it is skipped and the others still run].

**(b) AND/OR grouping.** No stage entry or objective in the masters mixes AND and OR in a
way a job can drive and see, so the check uses AI package conditions, which are evaluated
without dialogue (`EvaluatePackage`, then xNVSE's `GetCurrentPackage`; `q5b_trial_test`,
`q5b_trial_test2`, scenario `L5.quest.condition_groups`):
- GoodspringsFleePackage (0x00107480), the first package of three Goodsprings settlers:
  `VMS16.bGunFightStart == 1` (OR) / `VMS16b.bPowderGangAttacking == 1` /
  `VMS16.bTrudyHelp == 0`, i.e. A (OR) B, C. With A = 1, B = 0 and C false (bTrudyHelp 1)
  the settlers did **not** take it (they took their second packages, which need
  bGunFightStart 1); with C true they did; with B = 1 instead of A the same. So not
  `A OR (B AND C)`.
- SunnySmilesStayAtCurrentLocation (0x0010A21C), Sunny's third package (the two before it
  fail with `bMetSunny` 1 and VCG02 objective 10 not completed): objective 70 not shown,
  quest not completed, `GetStage VCG02 == 45` (OR) / `== 40` (OR) / `== 20`. At stage 20
  (and at 45) it was chosen with objective 70 hidden and **not** chosen with objective 70
  shown. Left to right (`((D AND Q AND S45) OR S40) OR S20`) would choose it; so not left
  to right.
- Together: a condition whose OR flag is set is ORed with the next one; such runs form
  groups, and the groups are ANDed: `(A OR B) AND C`, `D AND Q AND (S45 OR S40 OR S20)`.
  This is also how the masters' expression compiler orders `||` and `&&` (scripts.md §3.3).
  The run-on subject of these conditions is the package's actor; GetQuestVariable,
  GetStage, GetObjectiveDisplayed and GetQuestCompleted do not depend on it.

**(c) Stray Else/ElseIf/EndIf.** TecMineHostage (FalloutNV SCPT 0x00123191) compiles
`if GetDead == 0 … endif / elseif GetDead == 1 / activate / endif` in its OnActivate
block. Its only effect is that `Activate` (the default activation, a dead hostage's loot
menu), so it can be seen only when the block runs on a dead hostage. A scripted
`Activate player` never runs the block (finding f), so the first trials (dead hostage →
loot menu) showed nothing about the marker. With `Activate player 1` (`q5b_trial_test3`,
scenario `L5.quest.dead_activation`, both runs):
- a dead actor without an OnActivate block (a Techatticup legionary) → loot menu (default);
- ED-E's disabled body (dead; its OnActivate sets `vDialogueEDE.bEDEExamined` and shows a
  message, never calls Activate) → message box, variable 1, **no** loot menu: the block
  replaces the default on a dead actor too;
- the dead hostage → **loot menu**: the `activate` after the stray ElseIf ran;
- the living hostage with `Freed` 1 → nothing opens; with `Freed` 0 → its message box.
A clearer case is a quest script: BeatrixFX (0x0017BA11, script 0x0017B778, delay 0.1 s)
counts `timer` up with GetSecondsPassed; after `if timer > 1.5 && bump == 0 … endif` come
six `elseif timer > T && bump == n … endif` pairs (T = 2, 2.3, 2.5, 3.5, 4.5, 5.5) that set
`bump` to n + 1. After StartQuest, `bump` went 0 → 1 → 2 → … → 7, each step at the first
run past its threshold (timer 1.547, 2.062, 2.375, 2.547, 3.562, 4.547, 5.562 s), and at
9.078 s the closing `if timer > 9` stopped the quest (`q5b_trial_test3`; scenario
`L5.quest.stray_elseif`, both runs: values 0…7, then not running, `reset` 0, `bump` 7). So
an ElseIf reached after its chain was closed tests its condition and runs its body when
true. Whether a branch taken just before it would suppress it is not exercised (no two
thresholds fell in one run) [hypothesis: the marker acts as a new If].

**(d) Order and event timing.**
- GameMode on the frame of an activation: VCG01SkullBrahminActivator (Doc Mitchell's
  house; its GameMode counts `timer` down with GetSecondsPassed while `runTimer` is 1; its
  OnActivate block is empty), sampled every game loop, changed on every loop before,
  across and after `Activate player 1` (2–3 ticks of 1/64 s per loop; `q5b_trial_doc2`,
  `q5b_trial_doc3`): no run is skipped or doubled on the activation frame.
- OnActivate runs inside the Activate call: the house door's OnActivate
  (`set DocMitchellREF.PlayerLeft to 1`, then `activate`) had set the variable when it was
  read on the next line of the script that called `Activate player 1`, and the player was
  outside afterwards (scenario `L5.quest.ref_activation`, both runs).
- OnAdd and GameMode of an item: JarPistolPowder (OnAdd Player sets `killme`; GameMode
  adds 200 PowderPistol and calls RemoveMe). In the script that added the jar: jar 1,
  powder 0; at the first sample one frame later: jar 0, powder 200 (`q5b_trial_doc`;
  scenario `L5.quest.onadd_gamemode`, both runs). So an item's GameMode runs while it is
  in the player's inventory, and its OnAdd had run before that GameMode, at the latest in
  the same frame.
- OnLoad: the saloon jukebox (VintageRadioStartOnSCRIPT: OnLoad sets `init` to 1 when it
  is 0) and the broken radio (GoodspringsRadioScript: OnLoad calls `SetDestroyed 1`),
  disabled and enabled again while the player was in their cell: neither effect was there
  on the line after `Enable`; both were there at the first sample (`q5b_trial_test2`). An
  exterior case (VMS15's rising plane, enabled in its loaded cell) read `bLoaded` 1 already
  on the line after `Enable`, and its GameMode (`bLoaded` 1 → 2) did not run in 120 loops;
  not understood, not used.
- Quest scripts against object scripts within one frame: **not settled**. A job samples
  between frames, so the order shows only where a quest script and an object script write
  and read the same variable in one frame. The masters have no such pair that a job can
  drive without dialogue, message-box buttons or combat (searched: object scripts' GameMode
  blocks that read or write variables a quest script changes, and the reverse), and job
  scripts cannot attach scripts. The skull's object timer and VCG00's quest timer (both
  every frame) always dropped by the same amount in the same loop.

**(e) GetSecondsPassed after a gap.**
- Container menu: VCG00 (delay 0.01, so every frame; has a MenuMode block), VMQ03aTimer
  (delay 0.5; GameMode only) and the skull's object script, all counting down with
  GetSecondsPassed. While Doc's dresser's container menu was open (1,500 loops, 54 s of
  real time by the screenshot timestamps of `q5b_trial_doc3`) none of them changed; after
  `CloseAllMenus` the first drop was one ordinary frame for VCG00 and the skull (3/64 s,
  as before the menu) and 0.516 s for VMQ03aTimer (its normal 0.5 s). Scenario
  `L5.quest.seconds_after_menu` (900-frame menu): 0.046875 and 0.5. So the menu time is not
  counted at all (a cap would show as a larger, fixed value).
- Loading screen (`coc` to the saloon and back): VCG00 did not change for about 22 loops,
  then dropped by an ordinary frame (`q5b_trial_doc3`).
- The game clock (`GameDaysPassed`) advanced by 0.000118 days (10 game seconds at
  TimeScale 30, about 0.3 s of real time) during the 54 s container menu: the world clock
  stops in that menu too. (xnvse-capture.md §5 says the clock keeps running while the
  player is in menus; that was seen with the console and the Wait menu, not a container
  menu.)
- A Wait-menu gap was not captured (no unattended way to open the Wait menu; the
  `GameHour` substitute does not stop scripts).

**(f) Object scripts on references.**
- `rRef.GetVariable "name"` returns a variable of the reference's object script (the
  skull, Doc Mitchell, the hostages, the jukebox); a name the script does not declare
  returns 0; `rRef.HasVariable "name"` returns 1. `GetVariable "name" rRef` (the quest
  spelling of §1) returned 0 for a reference, and `rRef.timer` does not compile
  (`q5b_trial_doc`). `rRef.SetVariable "name" value` writes it (read back, and the
  skull's script counted down from the value written).
- **A scripted `ref.Activate player` does not run the target's OnActivate block**; it
  performs the default activation (a door moves the player, a corpse opens its loot menu,
  an activator does nothing). The door's OnActivate did not set `PlayerLeft`, dead ED-E's
  did not set `bEDEExamined` (`q5b_trial_test2`, `q5b_trial_doc2`; scenarios
  `L5.quest.dead_activation`, `L5.quest.ref_activation`). `ref.Activate player 1` runs the
  block instead (and the block's own `Activate` performs the default). This is the GECK
  wiki's RunOnActivateBlockFlag [documented].
- An actor's OnActivate block runs on a dead actor when it is run (flag set): ED-E's
  disabled body showed its message.

Other observations of this round:
- The game ran at about 25 loops per second in these sessions (the skull's per-loop drops
  of 2–3/64 s; round 1 saw about 60), so frame counts in jobs are not seconds.
- `$` of a number gives six significant digits (`99999.5`), not six decimals as §1 says;
  trial samples therefore encode countdowns as whole 1/64 s ticks.
- A container menu opened by a scripted Activate is open (`MenuMode 1008`) only one or two
  frames later; `CloseAllMenus` closes container menus and message boxes.

### 8.4 Predictions (written before each run; plans r2-p1 to r2-p4)

- Pa (all entries of ControlRumble's stage 0 run) — **held**.
- Pb (OR before AND for the flee package) — **held**; follow-up (not left to right, with
  Sunny's package) — **held**.
- Pc (a stray ElseIf reached in sequence is skipped) — **refuted** (BeatrixFX reaches 7;
  the dead hostage's flagged activation opened the loot menu).
- Pd (GameMode not skipped around an activation; OnActivate inside Activate; the jar
  replaced by the next sample) — **held**. OnLoad by the next frame, not inside Enable —
  **held** for the jukebox and the radio, **refuted** for the rising plane (unexplained).
- Pe (menu time not counted; made weakly, the alternative being the whole gap) — **held**.
- Pf (`rRef.GetVariable` works; `rRef.var` does not compile; the door's OnActivate sets
  PlayerLeft) — the first two **held**; the third **failed** until the run-OnActivate flag
  was used: a scripted Activate without it skips the block (not predicted).
- Scenario predictions (r2-p4) for the seven visible scenarios — all **held**.

### 8.5 Scenarios and goldens

`parity-lab/scenarios/layer5_quests2.json`, comparator keyed on `label` over
`/observations` (exact; `L5.quest.seconds_after_menu` with `abs_tol` 0.05 s); inputs
`parity-lab/inputs/layer5/quest2_<name>.actions.json`; goldens
`goldens/quest2/quest2_<name>.trace.json` (private). `L5.quest.multi_entry_stage` uses
only §4 operations; the others need the §8.2 additions, which the current candidate
rejects as unknown, so they **error** until 5B adds them (checked 2026-10-09 with this
branch's fnvtool: `python -m fnvlab run --scenario <id>` for each visible scenario →
`multi_entry_stage` passes, the six others error; the candidate was not run on the
holdouts).

| Scenario | Save | Rows | Notes |
|---|---|---|---|
| `L5.quest.multi_entry_stage` | nvftest | 2 | (a) |
| `L5.quest.condition_groups` | nvftest | 20 | (b) |
| `L5.quest.stray_elseif` | nvftest | 2 | (c) |
| `L5.quest.dead_activation` | nvftest | 9 | (c), (f) |
| `L5.quest.ref_activation` | nvfdoc | 5 | (d), (f) |
| `L5.quest.onadd_gamemode` | nvftest | 2 | (d) |
| `L5.quest.seconds_after_menu` | nvfdoc | 2 | (e); 0.05 s |
| `L5.quest.r2_holdout_a` | nvftest | — | **holdout** |
| `L5.quest.r2_holdout_b` | nvftest | — | **holdout** |

Stability: every scenario job ran twice, in separate game sessions (plans r2-p4 and
r2-p5). Every `given` matched the save in both runs (`golden` refuses otherwise), and the
second run scored 1.0 against the first under each scenario's comparator, holdouts
included. The only difference: VMQ03aTimer's drop after the menu, 0.5 then 0.484375
(inside the 0.05 s tolerance). The first run's traces are the goldens.

Rebuilding (owner's machine; both run locks first; never save in game): as §6, with
`make_inputs2.py`, a defs file for the quests the inputs name (ControlRumble, VTechatticup,
BeatrixFX, vDialogueEDE, VMS16, VMS16b, VFreeformGoodsprings, VCG02, VCG00, VMQ03aTimer)
and `-o C:/fnv-private/goldens/quest2/quest2_<name>.trace.json`.

### 8.6 Open items

- **VCG01 from a new game** still needs the owner's `nvfnew` save (§7).
- The order of quest and object scripts within a frame (§8.3 d).
- The rising plane's OnLoad and GameMode (§8.3 d).
- `L5.quest.condition_groups` needs AI package choice in the candidate (the conditions are
  Layer 5's, choosing a package is Layer 6's).
- A Wait-menu gap for GetSecondsPassed.
