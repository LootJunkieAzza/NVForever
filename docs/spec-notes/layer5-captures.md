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
