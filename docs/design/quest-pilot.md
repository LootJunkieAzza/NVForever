# Quest pilot: playing the walkthrough chains as a player

Issue #2365. Code: `engine/nvforever/src/pilot.rs` (the pilot), `crates/fnv-game/src/softlock.rs`
and `engine/nvf-play/src/softlock.rs` (the soft-lock watchdog). Everything here is [design]:
the original game has no such tool.

## 1. Why

The headless walkthroughs (`crates/fnv-game/walkthrough/*.actions.json`, run by
`fnv_game::runner`, `fnvtool script run`) reach about nine in ten of the quest stages the load
order can set, yet players hit soft locks in quests that pass headless. The chains stand in for
what breaks in play: travel (`move_player` puts the player there), AI arrivals (`package ... done`,
`X.MoveTo marker`), the player's dialogue picks (`say_info` says the answer directly), fights
(`kill`), activation reach, locked doors, terminals and items (`add_item`). The pilot plays the
same chains in the running game with the player character and leaves those stand-ins to the game.

## 2. Running it

```
NVFOREVER_HOME=<a folder of its own> nvforever --data <Data> --play --new-game \
  --pilot <chain> [--pilot <chain> ...] [--pilot-expect <headless trace> ...] \
  [--pilot-campaign] [--pilot-given] [--pilot-no-god] [--pilot-out <report.json>]
```

- `--pilot-expect`: the trace `fnvtool script run <Data> <plugins> --schema schemas/fnv
  --actions <chain>` prints, one per chain in the same order. Each labelled read is compared with
  the headless one of the same step; a `get_quest` that differs is read again for 45 s first.
- `--pilot-campaign` (with `--new-game` or `--load`): the chains go on from the game's own state.
  A chain's `given` quest entries are checked as preconditions (`precondition` findings), not set;
  the first chain's `given` is skipped. A save `pilot-after-<chain>` is written after each chain,
  so a later run can `--load` it and play only the chains after it.
- `--pilot-given` (with `--pilot-campaign`): a campaign's means, but each chain from its own
  `given`. For chains no campaign has reached yet; expect the false alarms a thin `given` brings
  (Doc Mitchell's OnActivate restarted the opening when VCG01 was only marked done).
- Set `NVFOREVER_HOME` to a folder of its own: the game autosaves after travel, and early runs
  overwrote the player's own autosave slot.
- Test aids the runs use: `NVF_PLAY_TEST_SCRIPT_MENUS=900`, `NVF_PLAY_AIDS_FROM_SCRIPTS=1`,
  `NVF_PLAY_TEST_UNATTENDED=1` (and `NVF_PLAY_TEST_INTRO_SKIP=1` from a new game).

## 3. What a step becomes

| step | in the pilot |
|---|---|
| `move_player`, `go_to_cell`, `trigger_enter` | a walk on the navmesh (`fnv_nav`) on the player's own movement controls, through doors; a long way out of doors is cut to its last 3,000 units (a fast-travel stand-in); never put where it could not walk |
| `talk`, `player_activate` | walk there, turn the view on it, ask the crosshair (`could_not_target` when it does not take it), activate as E does |
| `choose`, `wait_talk`, `end_talk` | the dialogue screen's own calls; leaving is by the Goodbye choice with its line heard out (#2157), Tab on a topic list, `no_way_out` otherwise |
| `say_info` (campaign) | the player brings the line about: keeps the speaker company, talks to it when it stands silent, picks the choices that lead to the line (`choice_toward`, up to 5 picks deep) |
| `kill` | a fight with the player's own weapon (a 9mm pistol is given once, god mode on by default); the body is searched for quest items and keys |
| `terminal` | walk there, activate, the screen's own input: password on the hacking page, into the sub menu, the entry |
| `add_item` to the player (campaign) | taken from whoever holds it in the loaded cells, or where it lies; `item_not_found` and the headless gift otherwise |
| `X.MoveTo marker` (campaign) | the player goes to the marker and waits there on the Wait menu, an hour at a time, up to 24 hours; `never_came` otherwise |
| `package`, `patrol_point`, `dialogue_package` | not carried out: the AI has to do it |
| `wait_menu`, `get_menus`, `press_button`, `dismiss_message_boxes` | the game's own menu; a box nobody waits for gets its last shown button after 1.5 s |
| anything else | the headless step on the running game, counted in the report's `headless` by kind |

Of its own accord, between steps: it fights what attacks the player or the actor it is with,
answers the level-up menu (points spread down the list, the first perk on offer), keeps its weapon
whole, searches bodies, and at a door that stays locked tries the door's switch (a script that names
the door), a talking activator beside it, any other scripted activator beside it, a terminal entry
that names the door, then another way round.

## 4. What it reports

`fnv.pilot.report/1`: steps (with errors), observations, divergences from the headless trace,
findings, the watchdog's soft-lock count, and `headless` (what was still done the headless way).
Finding kinds: `unreachable`, `stuck`, `could_not_target`, `no_way_out`, `line_not_said`,
`no_conversation`, `never_came`, `item_not_found`, `fight_not_won`, `wait_timed_out`,
`precondition`, `menu_closed`, `not_carried_out`, `step_timed_out`, `chain_abandoned`,
`pilot_held`.

Limits: a step 240 s; a fight 75 s; 12 failed steps in a row abandon a chain; 120 s of real time
in which the pilot cannot act end the run.

## 5. What it has found so far (2026-11-13/14)

- Hardin's power armour training holds the player for good (#2309): found by the watchdog.
- Every other stall so far was the pilot's own (no level-up answer, the wrong box button, a
  conversation broken off, a fight not fought). The places players report breaks (Hidden Valley,
  Freeside, the Strip) are where it is being pointed now.

## 7. Every chain at once

`tools/smoke/pilot/batch.py` runs every walkthrough chain (or the ones named) through the pilot
and ranks them:

```
python tools/smoke/pilot/batch.py --exe <nvforever.exe> --data <Data> --fnvtool <fnvtool.exe>     --home F:/nvf-pilot-home [--jobs 2] [--minutes 30] [--only beyond-the-beef gi-blues] [--skip-fail]
```

Each chain gets its headless trace (cached), a game of its own in `<home>/batch-<slot>` started from
`<home>/saves/pilot-after-intro.nvfsave` (a world past the opening; `--new-game` when it is not
there), `--pilot-campaign --pilot-given`, and at most `--minutes`. `summary.md` and `summary.json`
in `--out` (default `target/pilot-batch/`) list the chains worst first: soft locks, then findings
that stop a quest (`no_way_out`, `unreachable`, `never_came`, `chain_abandoned`), then the pilot's
own trouble; each row says what was still done the headless way. With 217 chains and two games at
once a full pass takes about a day; `--skip-fail` leaves out the 51 `fail-*` chains (a
quest's failure paths).

Read a finding against the chain's notes before calling it a bug: a chain run from its `given`
state carries the false alarms a thin `given` brings (§2).

## 6. Not yet

- A terminal opened from a message box button is refused in play (East Pump Station, Vault 34):
  being looked into separately.
- Melee, cover and reach as the NPCs judge them in fights; `hit` steps.
- A known-difference list, so that the opening's modal boxes stop showing as divergences.
- The static sweep (`fnvtool`, #2365 item 5) is its own work.
