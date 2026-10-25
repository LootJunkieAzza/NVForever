# CLAUDE.md — standing instructions for AI sessions on NVForever

NVForever is an open-source, clean-room reimplementation of the Fallout: New Vegas engine
(OpenMW-style). The owner wants sessions to work autonomously, including pushing.

The owner's playtest lists (all merged) and what is left after them are in
`docs/handoff/playtest-2026-10-12.md` (lists 1-3), `playtest-list-4.md` and `playtest-list-5.md`
(each with an Outcome section); the state of every system is in `docs/status.md`. The queue of
work that needs nothing from the owner, done one item at a time: `docs/handoff/queue-2026-10-20.md`.
What is left after that queue and the ESM gap audit: `docs/handoff/handover-2026-10-21.md`,
updated after audit groups 1–4 by `docs/handoff/handover-2026-10-22.md`. **Read first**: the
code-checked project audit `docs/handoff/project-audit-2026-10-23.md` (completion, playthrough
blockers, stale docs; its §7 is the ranked next work) and the owner's `playtest-list-7.md`.
What ESM data the engine still does nothing with, and the docs that no longer match the code:
`docs/handoff/esm-audit-2026-10-25.md`.

## Charter (non-negotiable)

- Clean room: describe formats/behaviour in plain language in `docs/spec-notes/`, implement
  from those notes. No leaked Bethesda/Gamebryo source. No DRM circumvention (the Steam
  `FalloutNV.exe` code section is encrypted; do not unpack it).
- Tag every factual claim `[documented]` (cite page/file), `[observed]` or `[hypothesis]`.
  Never invent offsets, opcodes or field meanings.
- Ship no game data. Goldens and game-derived dumps stay private and are never committed
  (`.gitignore` blocks game files).
- Work in layer order (docs/design/layer1.md … layer5.md; Layers 1–5 are done, Layers 6–7 are
  mostly built; what is complete, partly built and not started: docs/status.md).
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
python -m unittest discover -s tools/stress/tests
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
  safe, and runs up to 3 nvforever games on the machine at once (smoke slots,
  `tools/smoke/smoke/slots.py`; `--slots N` / `NVF_SMOKE_SLOTS`, `--jobs N` for this run;
  `--slots 1` is the same as `--exclusive`, the way to rerun a FAIL with the machine alone).
  While any slot runs, `.claude\smoke.lock` exists as a shared lock, so captures (which
  `mkdir` both game locks) wait; smoke never takes the capture lock `run.lock`, it only
  waits while it is held. Runners queue first-come, first-served (`smoke-queue` beside
  the first lock); the slots are dealt in queue order, and slots a run cannot use (its
  `--jobs`, or fewer launches) go to the runners behind it. `--exclusive` is the old way (both
  locks, one game on the machine) for a check that needs the screen to itself.
  It prints one line per scenario. **The summary is enough**: open a log in
  `target/smoke/` only for a FAIL, and then only the lines it quotes. `plan` shows what
  would run; `check` judges the saved logs again after you edit a scenario; `--no-merge`
  runs scenarios one by one. Environment instead of arguments: `NVF_EXE`, `NVF_DATA`,
  `NVF_FNVTOOL`, `NVF_SMOKE_LOCKS` (other machines), `NVF_OBSERVER_SAMPLES` (the AI observer
  captures).
- **Stop only your own runner, by the PID you started.** Never stop processes by a
  command-line match or with `taskkill /T` on a shell you did not start: on 2026-10-24 one
  such match stopped every session's smoke runner on the machine.
- Fix or explain every FAIL. KNOWN marks a check that fails on main for a stated reason;
  it does not fail the run, and a "now passes" line says the mark can go.
- **After merging anything into a branch: `python tools/smoke verify`** (fails when the
  merge fused, doubled or lost scenarios in scenarios.toml). Never `merge=union` the file;
  turn on its merge driver once per clone (tools/smoke/merge_driver.py). The script-stubs
  totals live in `tools/smoke/expected/script-stubs.toml`.
- **Adding a feature worth a smoke test: add a `[[scenario]]` to the file**, not prose here
  (expectation syntax: `tools/smoke/smoke/expect.py`). Never loosen an expectation to make
  a run pass; mark it `known_failing` with the reason instead.

## Commits

End commit messages with the session's attribution lines. Keep commits focused; write
what changed and what was verified.
