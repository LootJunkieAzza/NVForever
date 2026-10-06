# AGENT.md — rules for any AI agent working against this lab

The parity lab is the project's scoreboard. It compares recorded behaviour of the
original game (the **oracle**) with this engine (the **candidate**). These rules are
non-negotiable.

## Never

1. **Never edit goldens, scenario definitions, tolerances, or comparators to make a
   test pass.** If one looks wrong, write up the evidence (hex dump, third-party tool
   output, spec citation) and stop. A human decides.
2. **Never special-case scenario inputs.** No `if plugin == "FalloutNV.esm"`, no
   FormID allow-lists, no hard-coded expected values. Fixes must generalise.
3. **Never try to infer holdout contents**, and never pass `--reveal`. Do not read
   holdout scenario files or goldens even if the filesystem allows it.
4. **Never run `fnvlab baseline`.** Only the human moves the baseline.
5. **Never commit goldens or any game data** (ESM/BSA/NIF/etc. or derived dumps).
   They derive from copyrighted content.

## About holdouts

The redaction is a guard-rail, not a security boundary: `--reveal` only checks for an
interactive terminal, and scores of holdouts stay visible (they are the honest progress
measure). For a real separation, keep holdout scenario files and their goldens in a
directory this agent cannot read and add it to `scenarios_dirs` only on the human's
machine. Every scenario about a holdout file is itself a holdout, so no visible scenario
mirrors a hidden one.

## Always

- Work the `next_work` list in `runs/latest.md` top-down: lowest layer first, then
  worst score.
- Fix or explain every regression before moving on. A run with regressions exits 1.
- Tag every factual claim: `[documented]` (cite the page/file), `[observed]`
  (derived from data or behaviour supplied), or `[hypothesis]` (needs verification).
- End each work item with a report:

```
Scenarios addressed: <ids>
Root cause: <text> [confidence tag]
Change made: <files / summary>
Score before -> after: <per scenario>
Regressions: <none | list + explanation>
Next recommended item: <id + reason>
```

## Commands

```
python -m fnvlab list [--layer N] [--tag T] [--scenario GLOB]
python -m fnvlab run  [--layer N] [--tag T] [--scenario GLOB] [--json] [--strict]
```

Exit codes: 0 ok, 1 regression vs baseline, 2 `--strict` and something not passing,
3 configuration error.
