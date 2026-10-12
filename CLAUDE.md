# CLAUDE.md — standing instructions for AI sessions on NVForever

NVForever is an open-source, clean-room reimplementation of the Fallout: New Vegas engine
(OpenMW-style). The owner wants sessions to work autonomously, including pushing.

## Charter (non-negotiable)

- Clean room: describe formats/behaviour in plain language in `docs/spec-notes/`, implement
  from those notes. No leaked Bethesda/Gamebryo source. No DRM circumvention (the Steam
  `FalloutNV.exe` code section is encrypted; do not unpack it).
- Tag every factual claim `[documented]` (cite page/file), `[observed]` or `[hypothesis]`.
  Never invent offsets, opcodes or field meanings.
- Ship no game data. Goldens and game-derived dumps stay private and are never committed
  (`.gitignore` blocks game files).
- Work in layer order (docs/design/layer1.md, layer2.md, layer3.md; Layer 3 = world/rendering is in progress).
- nv-rs (slaterain/nv-rs) is a source of format facts only: never copy its code (its
  policy admits leaked Xbox symbols and translated decompiled code; owner decision
  2026-10-07). nif.xml is documentation only (no codegen) to keep the licence open.
- NVForever never writes into the game folder or `Documents\My Games\FalloutNV`
  (`crates/fnv-install`, docs/design/install-layout.md).
- Use the Rust ecosystem fully (owner decision); plan in docs/design/dependencies.md.

## Parity lab rules (parity-lab/AGENT.md)

Never edit goldens, scenarios, tolerances or comparators to make tests pass; never
special-case inputs; never read holdouts or pass `--reveal`; never run `fnvlab baseline`.
Fix or explain every regression.

## Checks before every push

Run only the ones your change calls for (`python tools/smoke plan` lists them; `run --checks`
runs them): root tests and clippy when `crates/` (or the root Cargo files, `schemas/`)
changed, engine clippy when `engine/` or `crates/` changed, a Python suite when its folder
changed.

```
cargo test --workspace --release
cargo clippy --workspace --all-targets --release -- -D warnings
python -m unittest discover -s parity-lab/tests
python -m unittest discover -s tools/capture/tests
python -m unittest discover -s tools/smoke/tests
(cd engine && cargo clippy --release --all-targets -- -D warnings)   # Bevy workspace (viewer + game)
```

With game data available (owner's install, read-only): build `fnvtool`, set
`parity-lab/lab.local.json`, run `python -m fnvlab run` and report the summary.
## Smoke tests: `tools/smoke` (game runs and fnvtool checks as data)

Every smoke test is a scenario in `tools/smoke/scenarios.toml`: the files that call for it,
the test aids, the arguments and the log lines expected. Run, after building nvforever (and
fnvtool, for the tool checks):

```
python tools/smoke run --exe <engine target>/release/nvforever.exe --data <Data> --fnvtool <root target>/release/fnvtool.exe
# (--changed origin/main is the default selection; add --checks to run the push checks it lists)
```

- It picks the scenarios whose paths your branch changed (`--only NAME…`, `--area A…`,
  `--all` otherwise), shares a game launch between scenarios where that was checked to be
  safe, takes both game locks for each launch and waits while another session holds them,
  and prints one line per scenario. **The summary is enough**: open a log in
  `target/smoke/` only for a FAIL, and then only the lines it quotes. `plan` shows what
  would run; `check` judges the saved logs again after you edit a scenario; `--no-merge`
  runs scenarios one by one. Environment instead of arguments: `NVF_EXE`, `NVF_DATA`,
  `NVF_FNVTOOL`, `NVF_SMOKE_LOCKS` (other machines), `NVF_OBSERVER_SAMPLES` (the AI observer
  captures).
- Fix or explain every FAIL. KNOWN marks a check that fails on main for a stated reason;
  it does not fail the run, and a "now passes" line says the mark can go.
- **Adding a feature worth a smoke test: add a `[[scenario]]` to the file**, not prose here
  (expectation syntax: `tools/smoke/smoke/expect.py`). Never loosen an expectation to make
  a run pass; mark it `known_failing` with the reason instead.

### Smoke tests still to convert into tools/smoke/scenarios.toml

Added to CLAUDE.md on main while the runner was being built (PR #226); convert each into a
scenario (same env, args and expectations) and delete it from here.

After changes to force greets, Follow/Accompany packages or MoveTo of actors (`fnv_game::Game::force_greet`,
`fnv_ai` `goal::Follow`, `Ai::relocate`, the engine's residents; actor-behaviour.md §5.1–§5.2), run Sunny's
walk to the wells with `NVF_PLAY_TEST_SCRIPT_MENUS=0.3`,
`NVF_PLAY_TEST_SCRIPT=14:activate:ProspectorSaloonDoor,1@VCG02=10:stage:VCG02:25,3@VCG02=10:activate:0x0010618E`,
`NVF_PLAY_TEST_TALK=0:*=VFreeformGoodspringsGSSunnySmilesTopic019` and `NVF_PLAY_TEST_GOTO=90:-67150:-9100:7650` set:
`engine/target/release/nvforever --data <Data> --cell Goodsprings --play --no-voice --hour 10 --run-seconds 150`
(expect exit 0, "CheyenneREF: walks after SunnyRef (CheyenneAccompany)", "SunnyRef: starts a conversation …
(VCG02SunnySmilesDialogueSneakStart), line 0x0010A1ED" and "conversation with SunnyRef ended (goodbye, 1 lines)"),
and the gas station, with `NVF_PLAY_TEST_SCRIPT_MENUS=0.3` and `NVF_PLAY_TEST_SCRIPT=3:start:VMS16,4:info:0x00105CC2:0x00104C7D`:
`engine/target/release/nvforever --data <Data> --cell GSGasStation --play --no-voice --hour 12 --run-seconds 30`
(expect exit 0, "SunnyRef: put at (512, -5971, 7554) in GSGasStation by a script (MoveTo)" and her "Time to look alive" line).
After changes to Detection lines or voices by distance (`fnv_game::awareness` barks, `nvf-play` `voice`; alert.md §4,
voice.md §9), run Cook-Cook's camp with `NVF_PLAY_TEST_GOD=1 NVF_PLAY_TEST_SCRIPT_MENUS=1`:
`engine/target/release/nvforever --data <Data> --cell Goodsprings --pos=-43200,73000,7000 --play --hour 12 --no-voice --run-seconds 45`
(expect exit 0, a few "alert: … says" lines, "held back by the group cooldown" above 0 in the alert summary, and
"voice: playing … [out of hearing" for lines from 2,000 units or more, with no subtitle for them).


## Commits

End commit messages with the session's attribution lines. Keep commits focused; write
what changed and what was verified.
